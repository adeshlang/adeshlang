use std::collections::HashMap;

use super::errors::{HttpError, HttpErrorKind};
use super::headers::Headers;
use super::structured_fields::{
    StructuredDictionary, StructuredFields, StructuredItem, StructuredValue,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Priority {
    pub urgency: u8,
    pub incremental: bool,
    pub extensions: HashMap<String, StructuredItem>,
}

impl Default for Priority {
    fn default() -> Self {
        Self {
            urgency: 3,
            incremental: false,
            extensions: HashMap::new(),
        }
    }
}

impl Priority {
    pub fn new(urgency: u8, incremental: bool) -> Result<Self, HttpError> {
        if urgency > 7 {
            return Err(HttpError::new(
                HttpErrorKind::ProtocolError,
                "Priority urgency must be between 0 and 7",
            ));
        }

        Ok(Self {
            urgency,
            incremental,
            extensions: HashMap::new(),
        })
    }

    pub fn parse(input: &str) -> Result<Self, HttpError> {
        let dict = StructuredFields::parse_dictionary(input)?;
        Self::from_dictionary(&dict)
    }

    pub fn encode(&self) -> String {
        let mut dict_entries = HashMap::new();
        dict_entries.insert(
            "u".to_string(),
            (
                StructuredValue::Item(StructuredItem::Integer(self.urgency as i64)),
                super::structured_fields::StructuredParameters::new(),
            ),
        );

        if self.incremental {
            dict_entries.insert(
                "i".to_string(),
                (
                    StructuredValue::Item(StructuredItem::Boolean(true)),
                    super::structured_fields::StructuredParameters::new(),
                ),
            );
        }

        for (k, v) in &self.extensions {
            dict_entries.insert(
                k.clone(),
                (
                    StructuredValue::Item(v.clone()),
                    super::structured_fields::StructuredParameters::new(),
                ),
            );
        }

        let dict = StructuredDictionary {
            entries: dict_entries,
        };
        StructuredFields::encode_dictionary(&dict)
    }

    pub fn from_headers(headers: &Headers) -> Result<Option<Self>, HttpError> {
        if let Some(raw) = headers.get("priority") {
            let parsed = Self::parse(raw)?;
            return Ok(Some(parsed));
        }
        Ok(None)
    }

    pub fn apply_to_headers(&self, headers: &mut Headers) -> Result<(), HttpError> {
        headers.insert("priority", &self.encode())
    }

    fn from_dictionary(dict: &StructuredDictionary) -> Result<Self, HttpError> {
        let mut priority = Priority::default();

        for (key, (value, _params)) in &dict.entries {
            match key.as_str() {
                "u" => {
                    let urgency = match value {
                        StructuredValue::Item(StructuredItem::Integer(v)) => *v,
                        _ => {
                            return Err(HttpError::new(
                                HttpErrorKind::ProtocolError,
                                "Priority urgency must be an integer",
                            ));
                        }
                    };

                    if !(0..=7).contains(&urgency) {
                        return Err(HttpError::new(
                            HttpErrorKind::ProtocolError,
                            "Priority urgency must be between 0 and 7",
                        ));
                    }
                    priority.urgency = urgency as u8;
                }
                "i" => {
                    priority.incremental = match value {
                        StructuredValue::Item(StructuredItem::Boolean(v)) => *v,
                        StructuredValue::Item(StructuredItem::Integer(v)) => *v != 0,
                        _ => {
                            return Err(HttpError::new(
                                HttpErrorKind::ProtocolError,
                                "Priority incremental must be a boolean",
                            ));
                        }
                    };
                }
                _ => {
                    if let StructuredValue::Item(item) = value {
                        priority.extensions.insert(key.clone(), item.clone());
                    }
                }
            }
        }

        Ok(priority)
    }
}
