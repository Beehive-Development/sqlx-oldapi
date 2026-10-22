use std::io::Read;

use bytes::{Buf, Bytes};

use crate::error::Error;
use crate::mssql::io::MssqlBufExt;

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum EnvChange {
    Database(String),
    Language(String),
    CharacterSet(String),
    PacketSize(u32),
    UnicodeDataSortingLocalId(String),
    UnicodeDataSortingComparisonFlags(String),
    SqlCollation(Bytes),

    // TDS 7.2+
    BeginTransaction(u64),
    CommitTransaction(u64),
    RollbackTransaction(u64),
    EnlistDtcTransaction,
    DefectTransaction,
    RealTimeLogShipping,
    PromoteTransaction,
    TransactionManagerAddress,
    TransactionEnded,
    ResetConnectionCompletionAck,
    LoginRequestUserNameAck,

    // TDS 7.4+
    RoutingInformation(String, u16),
}

impl EnvChange {
    pub(crate) fn get(buf: &mut Bytes) -> Result<Self, Error> {
        let len = buf.get_u16_le();
        let ty = buf.get_u8();
        let mut data = buf.split_to((len - 1) as usize);

        Ok(match ty {
            1 => EnvChange::Database(data.get_b_varchar()?),
            2 => EnvChange::Language(data.get_b_varchar()?),
            3 => EnvChange::CharacterSet(data.get_b_varchar()?),
            4 => {
                let size_str = data.get_b_varchar()?;
                let size = size_str
                    .parse()
                    .map_err(|e| err_protocol!("Invalid packet size {}: {}", size_str, e))?;
                EnvChange::PacketSize(size)
            }
            5 => EnvChange::UnicodeDataSortingLocalId(data.get_b_varchar()?),
            6 => EnvChange::UnicodeDataSortingComparisonFlags(data.get_b_varchar()?),
            7 => EnvChange::SqlCollation(data.get_b_varbyte()),
            8 => EnvChange::BeginTransaction(data.get_b_varbyte().get_u64_le()),

            9 => {
                let _ = data.get_u8();
                EnvChange::CommitTransaction(data.get_u64_le())
            }

            10 => {
                let _ = data.get_u8();
                EnvChange::RollbackTransaction(data.get_u64_le())
            }

            20 => {
                log::trace!(
                    "encountered `EnvChange::RoutingInformation`, data: {:?}",
                    data.to_vec(),
                );

                let routing_information_length = data.get_u16_le();

                let mut routing_information = vec![0; routing_information_length as usize];
                data.copy_to_slice(&mut routing_information);

                let (protocol, rest) = routing_information.split_first_chunk::<3>().unwrap();
                let mut protocol = protocol as &[u8];

                assert_eq!(protocol.get_u8(), 0);
                let protocol_property = protocol.get_u16_le();

                let (length, rest) = rest.split_first_chunk::<2>().unwrap();
                let length = u16::from_le_bytes([length[0], length[1]]);

                let (characters, rest) = rest.split_at_checked(length as usize * 2).unwrap();

                let characters = characters
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|short| u16::from_le_bytes([short[0], short[1]]))
                    .collect::<Vec<_>>();

                let alternate_server = String::from_utf16(&characters).unwrap();

                log::trace!(
                    "parsed `EnvChange::RoutingInformation`, alternate_server: {}, protocol_property: {}",
                    alternate_server,
                    protocol_property
                );

                EnvChange::RoutingInformation(alternate_server, protocol_property)
            }

            _ => {
                return Err(err_protocol!("unexpected value {} for ENVCHANGE Type", ty));
            }
        })
    }
}
