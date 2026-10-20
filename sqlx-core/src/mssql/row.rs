use crate::column::ColumnIndex;
use crate::error::Error;
use crate::ext::ustr::UStr;
use crate::mssql::protocol::row::Row as ProtocolRow;
use crate::mssql::{Mssql, MssqlColumn, MssqlValueRef};
use crate::row::Row;
use crate::HashMap;
use std::sync::Arc;

pub struct MssqlRow {
    pub(crate) row: ProtocolRow,
    pub(crate) columns: Arc<Vec<MssqlColumn>>,
    pub(crate) column_names: Arc<HashMap<UStr, usize>>,
}

impl crate::row::private_row::Sealed for MssqlRow {}

impl Row for MssqlRow {
    type Database = Mssql;

    fn columns(&self) -> &[MssqlColumn] {
        &self.columns
    }

    fn try_get_raw<I>(&self, index: I) -> Result<MssqlValueRef<'_>, Error>
    where
        I: ColumnIndex<Self>,
    {
        let index = index.index(self)?;
        let value = MssqlValueRef {
            data: self.row.values[index].as_ref(),
            type_info: self.row.column_types[index].clone(),
        };

        Ok(value)
    }
}

impl ColumnIndex<MssqlRow> for &'_ str {
    fn index(&self, row: &MssqlRow) -> Result<usize, Error> {
        row.column_names
            .get(*self)
            .ok_or_else(|| Error::ColumnNotFound((*self).into()))
            .copied()
    }
}

#[cfg(feature = "any")]
impl From<MssqlRow> for crate::any::AnyRow {
    #[inline]
    fn from(row: MssqlRow) -> Self {
        crate::any::AnyRow {
            columns: row.columns.iter().map(|col| col.clone().into()).collect(),
            kind: crate::any::row::AnyRowKind::Mssql(row),
        }
    }
}

/// Utility builder for a [`MssqlRow`].
#[derive(Debug, Default)]
pub struct MssqlRowBuilder {
    columns: Vec<MssqlColumn>,
    values: Vec<Option<bytes::Bytes>>,
    column_types: Vec<crate::mssql::MssqlTypeInfo>,
}

impl MssqlRowBuilder {
    /// Add a [`MssqlColumn`] encoded with the given `value` to the builder.
    #[must_use]
    pub fn column<'q, T>(self, name: &str, value: T) -> Self
    where
        T: crate::encode::Encode<'q, Mssql> + crate::types::Type<Mssql>,
    {
        let type_info = value.produces().unwrap_or_else(T::type_info);
        let mut buf = Vec::new();

        let encoded = match value.encode(&mut buf) {
            crate::encode::IsNull::Yes => None,
            crate::encode::IsNull::No => Some(bytes::Bytes::from(buf)),
        };

        self.push(name, type_info, encoded)
    }

    /// Add a [`MssqlColumn`] encoded with a `null` value to the builder.
    #[must_use]
    pub fn null_column<T: crate::types::Type<Mssql>>(self, name: &str) -> Self {
        self.push(name, T::type_info(), None)
    }

    // Pushes the _raw_ values of a [`MssqlColumn`] to the builder.
    fn push(
        mut self,
        name: &str,
        type_info: crate::mssql::MssqlTypeInfo,
        value: Option<bytes::Bytes>,
    ) -> Self {
        self.columns.push(MssqlColumn {
            ordinal: self.columns.len(),
            name: UStr::new(name),
            type_info: type_info.clone(),
            flags: crate::mssql::protocol::col_meta_data::Flags::NULLABLE,
        });

        self.column_types.push(type_info);
        self.values.push(value);
        self
    }

    /// Consumes the builder and returns the finished [`MssqlRow`].
    #[must_use]
    pub fn build(self) -> MssqlRow {
        let column_names = self
            .columns
            .iter()
            .map(|column| (column.name.clone(), column.ordinal))
            .collect::<HashMap<_, _>>();

        MssqlRow {
            row: ProtocolRow {
                column_types: self.column_types,
                values: self.values,
            },
            columns: Arc::new(self.columns),
            column_names: Arc::new(column_names),
        }
    }
}
