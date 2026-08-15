use serde_json::Value;

/// Generic parsed field representation distinguishing absent, null, wrong-type,
/// and present states.
#[derive(Clone, Debug)]
pub enum ParsedField<T> {
    Absent,
    Null,
    WrongType(String),
    Present(T),
}

pub type StringField = ParsedField<String>;
pub type StringArrayField = ParsedField<Vec<String>>;

impl<T> ParsedField<T> {
    pub(super) fn ok(self) -> Option<T> {
        match self {
            Self::Present(value) => Some(value),
            _ => None,
        }
    }

    pub(super) fn from_value_opt(value: Option<&Value>) -> Self
    where
        Self: FromValue,
    {
        value.map_or(Self::Absent, Self::from_value)
    }
}

pub(super) trait FromValue: Sized {
    fn from_value(value: &Value) -> Self;
}

impl FromValue for ParsedField<String> {
    fn from_value(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::String(value) => Self::Present(value.clone()),
            other => Self::WrongType(format!("expected string, got {}", type_name(other))),
        }
    }
}

impl FromValue for ParsedField<Vec<String>> {
    fn from_value(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Array(values) => {
                let mut items = Vec::with_capacity(values.len());
                for value in values {
                    match value.as_str() {
                        Some(value) => items.push(value.to_owned()),
                        None => {
                            return Self::WrongType(format!(
                                "expected string element in array, got {}",
                                type_name(value)
                            ));
                        }
                    }
                }
                Self::Present(items)
            }
            other => Self::WrongType(format!("expected array, got {}", type_name(other))),
        }
    }
}

pub(super) fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub(super) trait FieldState {
    fn error(&self) -> Option<String>;
}

impl<T> FieldState for ParsedField<T> {
    fn error(&self) -> Option<String> {
        match self {
            Self::WrongType(message) => Some(message.clone()),
            Self::Null => Some("value is null".into()),
            Self::Absent | Self::Present(_) => None,
        }
    }
}
