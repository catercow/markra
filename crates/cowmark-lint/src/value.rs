//! Values as JavaScript sees them after `toJS()` / `JSON.parse`, so comparisons and messages
//! match the editor's TypeScript linter.

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    /// Keys in insertion order.
    Obj(Vec<(String, Value)>),
}

impl Value {
    /// `String(value)` in JavaScript.
    pub fn js_string(&self) -> String {
        match self {
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Num(n) => js_number(*n),
            Value::Str(s) => s.clone(),
            Value::Arr(items) => items
                .iter()
                .map(|item| match item {
                    Value::Null => String::new(),
                    other => other.js_string(),
                })
                .collect::<Vec<_>>()
                .join(","),
            Value::Obj(_) => "[object Object]".into(),
        }
    }

    pub fn is_nullish(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn from_json(value: serde_json::Value) -> Value {
        match value {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(b),
            serde_json::Value::Number(n) => Value::Num(n.as_f64().unwrap_or(f64::NAN)),
            serde_json::Value::String(s) => Value::Str(s),
            serde_json::Value::Array(items) => {
                Value::Arr(items.into_iter().map(Value::from_json).collect())
            }
            serde_json::Value::Object(map) => Value::Obj(
                map.into_iter()
                    .map(|(k, v)| (k, Value::from_json(v)))
                    .collect(),
            ),
        }
    }
}

/// `String(number)` for the numbers content uses.
pub fn js_number(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if n == n.trunc() && n.abs() < 1e21 {
        return format!("{}", n as i128);
    }
    format!("{n}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_like_javascript() {
        assert_eq!(Value::Num(11.5).js_string(), "11.5");
        assert_eq!(Value::Num(100.0).js_string(), "100");
        assert_eq!(Value::Num(-0.0).js_string(), "0");
        assert_eq!(
            Value::Arr(vec![Value::Num(1.0), Value::Str("a".into())]).js_string(),
            "1,a"
        );
    }
}
