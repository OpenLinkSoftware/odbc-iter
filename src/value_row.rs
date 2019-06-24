use crate::value::{TryFromValue, Value, ValueType};
use std::convert::Infallible;
use std::error::Error;
use std::fmt;

/// Row of dynamic nullable column values.
/// 
/// This objects are constructed from row data returned by ODBC library and can be further converted to types implementing `TryFromValueRow`/`TryFromValue` traits.
pub type ValueRow = Vec<Option<Value>>;

/// Description of column type, name and nullability properties used to represent row schema.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnType {
    pub value_type: ValueType,
    pub nullable: bool,
    pub name: String,
}

// Note: TryFrom/TryInto cannot be implemented since we need to own the trait

/// This traits allow for conversion of `ValueRow` type used internally by `ResultSet` iterator to any
/// other type returned as `Item` that implements it.
/// 
/// This trait is implemented for Rust tuple type enabling conversion of rows to tuples of types implementing `TryFromValue`.
/// Also this trait implementation allows to convert single column rows to types implementing `TryFromValue`.
/// 
/// This trait can be implemented for custom objects. This will enable them to be queried directly from database as `Item` of `ResultSet` iterator.
pub trait TryFromValueRow: Sized {
    type Error: Error + 'static;
    /// Given `ColumnType` convert from `ValueRow` to other type of value representing table row.
    fn try_from_row<'n>(values: ValueRow, schema: &'n [ColumnType]) -> Result<Self, Self::Error>;
}

/// Errors that may happen during conversion of `ValueRow` to given type.
#[derive(Debug)]
pub enum RowConvertError {
    UnexpectedNullValue(&'static str),
    UnexpectedValue,
    UnexpectedNumberOfColumns { expected: u16, got: usize },
    ValueConvertError(Box<dyn Error>),
}

impl fmt::Display for RowConvertError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RowConvertError::UnexpectedNullValue(t) => {
                write!(f, "expecting value of type {} but got NULL", t)
            }
            RowConvertError::UnexpectedValue => write!(f, "expecting no data (unit) but got a row"),
            RowConvertError::UnexpectedNumberOfColumns { expected, got } => write!(
                f,
                "unexpected number of columns: expected {} but got {}",
                expected, got
            ),
            RowConvertError::ValueConvertError(_) => {
                write!(f, "failed to convert column value to target type")
            }
        }
    }
}

impl Error for RowConvertError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RowConvertError::UnexpectedNullValue(_)
            | RowConvertError::UnexpectedValue
            | RowConvertError::UnexpectedNumberOfColumns { .. } => None,
            RowConvertError::ValueConvertError(err) => Some(err.as_ref()),
        }
    }
}

/// Allow to retrieve unconverted `ValueRow` as item of `ResultSet` iterator.
impl TryFromValueRow for ValueRow {
    type Error = Infallible;
    fn try_from_row<'n>(values: ValueRow, _schema: &'n [ColumnType]) -> Result<Self, Self::Error> {
        Ok(values)
    }
}

/// Unit can be used to signal that no rows of data should be produced.
impl TryFromValueRow for () {
    type Error = RowConvertError;
    fn try_from_row<'n>(_values: ValueRow, _schema: &'n [ColumnType]) -> Result<Self, Self::Error> {
        Err(RowConvertError::UnexpectedValue)
    }
}

/// Convert row with single column to any type implementing `TryFromValue`.
impl<T> TryFromValueRow for T
where
    T: TryFromValue,
{
    type Error = RowConvertError;
    fn try_from_row<'n>(
        mut values: ValueRow,
        _schema: &'n [ColumnType],
    ) -> Result<Self, Self::Error> {
        if values.len() != 1 {
            return Err(RowConvertError::UnexpectedNumberOfColumns {
                expected: 1,
                got: values.len(),
            });
        }
        values
            .pop()
            .ok_or_else(|| RowConvertError::UnexpectedNullValue("Value"))
            .and_then(|v| {
                TryFromValue::try_from_value(v)
                    .map_err(|e| RowConvertError::ValueConvertError(Box::new(e)))
            })
    }
}

/// Errors that my arise when converting rows to tuples.
#[derive(Debug)]
pub enum RowConvertTupleError {
    UnexpectedNumberOfColumns { expected: u16, tuple: &'static str },
    ValueConvertError(Box<dyn Error>),
}

impl fmt::Display for RowConvertTupleError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RowConvertTupleError::UnexpectedNumberOfColumns { expected, tuple } => write!(
                f,
                "failed to convert row with {} columns to tuple {}",
                expected, tuple
            ),
            RowConvertTupleError::ValueConvertError(_) => {
                write!(f, "failed to convert column value to target type")
            }
        }
    }
}

impl Error for RowConvertTupleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            RowConvertTupleError::UnexpectedNumberOfColumns { .. } => None,
            RowConvertTupleError::ValueConvertError(err) => Some(err.as_ref()),
        }
    }
}

macro_rules! count {
    () => (0usize);
    ( $x:tt $($xs:tt)* ) => (1usize + count!($($xs)*));
}

macro_rules! try_from_tuple {
    ($(
        $Tuple:ident {
            $(($idx:tt) -> $T:ident)+
        }
    )+) => {
        $(
            impl<$($T: TryFromValue),+> TryFromValueRow for ($($T,)+) {
                type Error = RowConvertTupleError;
                fn try_from_row<'n>(values: ValueRow, _schema: &'n [ColumnType]) -> Result<($($T,)+), Self::Error> {
                    if values.len() != count!($($T)+) {
                        return Err(RowConvertTupleError::UnexpectedNumberOfColumns { expected: values.len() as u16, tuple: stringify![($($T,)+)] })
                    }
                    let mut values = values.into_iter();
                    Ok(($({ let x: $T = $T::try_from_value(values.next().unwrap()).map_err(|err| RowConvertTupleError::ValueConvertError(Box::new(err)))?; x},)+))
                }
            }
        )+
    }
}

try_from_tuple! {
    Tuple1 {
        (0) -> A
    }
    Tuple2 {
        (0) -> A
        (1) -> B
    }
    Tuple3 {
        (0) -> A
        (1) -> B
        (2) -> C
    }
    Tuple4 {
        (0) -> A
        (1) -> B
        (2) -> C
        (3) -> D
    }
    Tuple5 {
        (0) -> A
        (1) -> B
        (2) -> C
        (3) -> D
        (4) -> E
    }
    Tuple6 {
        (0) -> A
        (1) -> B
        (2) -> C
        (3) -> D
        (4) -> E
        (5) -> F
    }
    Tuple7 {
        (0) -> A
        (1) -> B
        (2) -> C
        (3) -> D
        (4) -> E
        (5) -> F
        (6) -> G
    }
    Tuple8 {
        (0) -> A
        (1) -> B
        (2) -> C
        (3) -> D
        (4) -> E
        (5) -> F
        (6) -> G
        (7) -> H
    }
    Tuple9 {
        (0) -> A
        (1) -> B
        (2) -> C
        (3) -> D
        (4) -> E
        (5) -> F
        (6) -> G
        (7) -> H
        (8) -> I
    }
    Tuple10 {
        (0) -> A
        (1) -> B
        (2) -> C
        (3) -> D
        (4) -> E
        (5) -> F
        (6) -> G
        (7) -> H
        (8) -> I
        (9) -> J
    }
    Tuple11 {
        (0) -> A
        (1) -> B
        (2) -> C
        (3) -> D
        (4) -> E
        (5) -> F
        (6) -> G
        (7) -> H
        (8) -> I
        (9) -> J
        (10) -> K
    }
    Tuple12 {
        (0) -> A
        (1) -> B
        (2) -> C
        (3) -> D
        (4) -> E
        (5) -> F
        (6) -> G
        (7) -> H
        (8) -> I
        (9) -> J
        (10) -> K
        (11) -> L
    }
}

//TODO: this tests should not need DB
#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use crate::Odbc;
    #[allow(unused_imports)]
    use assert_matches::assert_matches;

    #[derive(Debug)]
    struct Foo {
        val: i64,
    }

    impl TryFromValueRow for Foo {
        type Error = Infallible;
        fn try_from_row<'n>(
            mut values: ValueRow,
            _schema: &'n [ColumnType],
        ) -> Result<Self, Self::Error> {
            Ok(values
                .pop()
                .map(|val| Foo {
                    val: val.and_then(|v| v.to_i64()).expect("val to be an bigint"),
                })
                .expect("value"))
        }
    }

    use crate::value::Value;

    #[test]
    #[cfg(feature = "test-monetdb")]
    fn test_custom_type() {
        let mut db = crate::tests::connect_monetdb();

        let foo: Foo = db
            .handle()
            .query("SELECT CAST(42 AS BIGINT) AS val;")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(foo.val, 42);
    }

    #[test]
    #[cfg(feature = "test-monetdb")]
    fn test_single_value() {
        let mut db = crate::tests::connect_monetdb();

        let value: Value = db
            .handle()
            .query("SELECT CAST(42 AS BIGINT)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(value.to_i64().unwrap(), 42);
    }

    #[test]
    #[cfg(feature = "test-monetdb")]
    fn test_single_nullable_value() {
        let mut db = crate::tests::connect_monetdb();;

        let value: Option<Value> = db
            .handle()
            .query("SELECT CAST(42 AS BIGINT)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert!(value.is_some());
        assert_eq!(value.unwrap().to_i64().unwrap(), 42);

        let value: Option<Value> = db
            .handle()
            .query("SELECT CAST(NULL AS BIGINT)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert!(value.is_none());
    }

    #[test]
    #[cfg(feature = "test-monetdb")]
    fn test_value_row() {
        let mut db = crate::tests::connect_monetdb();

        let value: ValueRow = db
            .handle()
            .query("SELECT CAST(42 AS BIGINT), CAST(22 AS INTEGER)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(value.len(), 2);
        assert_eq!(value[0].as_ref().unwrap().to_i64().unwrap(), 42);
        assert_eq!(value[1].as_ref().unwrap().to_i32().unwrap(), 22);
    }

    #[test]
    #[cfg(feature = "test-monetdb")]
    fn test_single_copy() {
        let mut db = crate::tests::connect_monetdb();

        let value: bool = db
            .handle()
            .query("SELECT true")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(value, true);

        let value: Option<bool> = db
            .handle()
            .query("SELECT true")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(value.unwrap(), true);

        let value: Option<bool> = db
            .handle()
            .query("SELECT CAST(NULL AS BOOL)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert!(value.is_none());

        let value: i64 = db
            .handle()
            .query("SELECT CAST(42 AS BIGINT)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(value, 42);

        let value: Option<i64> = db
            .handle()
            .query("SELECT CAST(42 AS BIGINT)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(value.unwrap(), 42i64);

        let value: Option<i64> = db
            .handle()
            .query("SELECT CAST(NULL AS BIGINT)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert!(value.is_none());
    }

    #[test]
    #[cfg(feature = "test-monetdb")]
    fn test_single_unsigned() {
        let mut db = crate::tests::connect_monetdb();

        let value: Option<u64> = db
            .handle()
            .query("SELECT CAST(42 AS BIGINT)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(value.unwrap(), 42u64);
    }

    #[test]
    #[cfg(feature = "test-monetdb")]
    #[should_panic(expected = "ValueOutOfRange")]
    fn test_single_unsigned_err() {
        let mut db = crate::tests::connect_monetdb();

        let _value: Option<u64> = db
            .handle()
            .query("SELECT CAST(-666 AS BIGINT)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");
    }

    #[test]
    #[cfg(feature = "test-monetdb")]
    fn test_single_string() {
        let mut db = crate::tests::connect_monetdb();

        let value: String = db
            .handle()
            .query("SELECT 'foo'")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(&value, "foo");

        let value: Option<String> = db
            .handle()
            .query("SELECT 'foo'")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(&value.unwrap(), "foo");

        let value: Option<String> = db
            .handle()
            .query("SELECT CAST(NULL AS STRING)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert!(value.is_none());
    }

    #[test]
    #[cfg(feature = "test-monetdb")]
    fn test_single_date() {
        use chrono::Datelike;
        use chrono::NaiveDate;

        let mut db = crate::tests::connect_monetdb();

        let value: NaiveDate = db
            .handle()
            .query("SELECT CAST('2019-04-02' AS DATE)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(value.year(), 2019);
        assert_eq!(value.month(), 4);
        assert_eq!(value.day(), 2);

        let value: Option<NaiveDate> = db
            .handle()
            .query("SELECT CAST('2019-04-02' AS DATE)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(value.unwrap().year(), 2019);
        assert_eq!(value.unwrap().month(), 4);
        assert_eq!(value.unwrap().day(), 2);

        let value: Option<NaiveDate> = db
            .handle()
            .query("SELECT CAST(NULL AS DATE)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert!(value.is_none());
    }

    #[test]
    #[cfg(feature = "test-monetdb")]
    fn test_tuple_value() {
        let mut db = crate::tests::connect_monetdb();

        let value: (String, i64, bool) = db
            .handle()
            .query("SELECT 'foo', CAST(42 AS BIGINT), true")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(&value.0, "foo");
        assert_eq!(value.1, 42);
        assert_eq!(value.2, true);

        let value: (Option<String>, i64, Option<bool>) = db
            .handle()
            .query("SELECT 'foo', CAST(42 AS BIGINT), true")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert_eq!(&value.0.unwrap(), "foo");
        assert_eq!(value.1, 42);
        assert_eq!(value.2.unwrap(), true);

        let value: (Option<String>, i64, Option<bool>) = db
            .handle()
            .query("SELECT CAST(NULL AS STRING), CAST(42 AS BIGINT), CAST(NULL AS BOOL)")
            .expect("failed to run query")
            .single()
            .expect("fetch data");

        assert!(&value.0.is_none());
        assert_eq!(value.1, 42);
        assert!(value.2.is_none());
    }

    #[test]
    fn test_value_row_conversions() {
        let test_row: ValueRow = vec![Some(Value::Bit(true)), Some(Value::Integer(42)), None];
        type Test = (Option<bool>, Option<u32>, Option<String>);

        //let (b, u, s): (Option<bool>, Option<u32>, Option<String>) = test_row.try_into().unwrap();
        let (b, u, s) = Test::try_from_row(test_row, &[]).unwrap();
        assert_eq!(b, Some(true));
        assert_eq!(u, Some(42));
        assert_eq!(s, None);
    }
}