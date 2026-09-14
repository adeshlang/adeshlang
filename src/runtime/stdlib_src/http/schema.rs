//! Core Schema, DTO Validation & Problem Details Engine for AdeshLang HTTP Standard Library.

use crate::parsing::ast::Value;
use crate::runtime::stdlib_src::http::domain_types::DomainTypeKind;
use crate::utils::collections::FastMap;
use std::sync::Arc;

/// Validation evaluation mode for unknown properties
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DTOMode {
    /// Reject any properties not explicitly declared in the schema (Secure default)
    Strict,
    /// Silently strip unknown properties during validation/construction
    StripUnknown,
    /// Preserve unknown properties as-is
    Passthrough,
}

/// Field constraint declarations
#[derive(Debug, Clone)]
pub enum Constraint {
    Min(f64),
    Max(f64),
    MinLength(usize),
    MaxLength(usize),
    ExactLength(usize),
    Pattern(String),
    Domain(DomainTypeKind),
}

/// Field Type Specification
#[derive(Debug, Clone)]
pub enum FieldType {
    String,
    Int,
    U32,
    Float,
    Bool,
    Domain(DomainTypeKind),
    Enum(Vec<String>),
    Array(Box<FieldType>),
    Map(Box<FieldType>),
    Object(Arc<Schema>),
    Any,
}

/// Individual Field Specification in a DTO Schema
#[derive(Debug, Clone)]
pub struct FieldSpec {
    pub name: String,
    pub field_type: FieldType,
    pub is_required: bool,
    pub is_nullable: bool,
    pub default_value: Option<Value>,
    pub is_secret: bool,
    pub is_write_only: bool,
    pub is_read_only: bool,
    pub constraints: Vec<Constraint>,
}

impl FieldSpec {
    pub fn new(name: impl Into<String>, field_type: FieldType) -> Self {
        Self {
            name: name.into(),
            field_type,
            is_required: true,
            is_nullable: false,
            default_value: None,
            is_secret: false,
            is_write_only: false,
            is_read_only: false,
            constraints: Vec::new(),
        }
    }

    pub fn optional(mut self) -> Self {
        self.is_required = false;
        self
    }

    pub fn nullable(mut self) -> Self {
        self.is_nullable = true;
        self
    }

    pub fn default_val(mut self, val: Value) -> Self {
        self.default_value = Some(val);
        self.is_required = false;
        self
    }

    pub fn secret(mut self) -> Self {
        self.is_secret = true;
        self
    }

    pub fn write_only(mut self) -> Self {
        self.is_write_only = true;
        self
    }

    pub fn read_only(mut self) -> Self {
        self.is_read_only = true;
        self
    }

    pub fn add_constraint(mut self, constraint: Constraint) -> Self {
        self.constraints.push(constraint);
        self
    }
}

/// Structured Individual Field Error Item
#[derive(Debug, Clone)]
pub struct FieldError {
    pub path: Vec<String>,
    pub code: String,
    pub message: String,
    pub expected: String,
    pub received: String,
}

impl FieldError {
    pub fn to_value(&self) -> Value {
        let mut map = FastMap::default();
        let path_arr: Vec<Value> = self.path.iter().map(|s| Value::Str(s.clone())).collect();
        map.insert("path".to_string(), Value::Array(path_arr));
        map.insert("code".to_string(), Value::Str(self.code.clone()));
        map.insert("message".to_string(), Value::Str(self.message.clone()));
        map.insert("expected".to_string(), Value::Str(self.expected.clone()));
        map.insert("received".to_string(), Value::Str(self.received.clone()));
        Value::Object(Arc::new(map))
    }
}

/// First-class ValidationError collection
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
    pub errors: Vec<FieldError>,
}

impl ValidationError {
    pub fn new(msg: impl Into<String>, errors: Vec<FieldError>) -> Self {
        Self {
            message: msg.into(),
            errors,
        }
    }

    pub fn to_value(&self) -> Value {
        let mut map = FastMap::default();
        map.insert(
            "name".to_string(),
            Value::Str("ValidationError".to_string()),
        );
        map.insert("message".to_string(), Value::Str(self.message.clone()));
        let err_arr: Vec<Value> = self.errors.iter().map(|e| e.to_value()).collect();
        map.insert("errors".to_string(), Value::Array(err_arr));
        Value::Object(Arc::new(map))
    }

    /// Renders RFC 9457 Problem Details object format (HTTP 422 Unprocessable Entity / 400 Bad Request)
    pub fn to_problem_details(&self, status_code: u16, instance_uri: &str) -> Value {
        let mut map = FastMap::default();
        map.insert(
            "type".to_string(),
            Value::Str("https://adesh.dev/errors/validation-failed".to_string()),
        );
        map.insert(
            "title".to_string(),
            Value::Str("Validation Failed".to_string()),
        );
        map.insert("status".to_string(), Value::I64(status_code as i64));
        map.insert("detail".to_string(), Value::Str(self.message.clone()));
        map.insert("instance".to_string(), Value::Str(instance_uri.to_string()));
        let err_arr: Vec<Value> = self.errors.iter().map(|e| e.to_value()).collect();
        map.insert("invalidParams".to_string(), Value::Array(err_arr));
        Value::Object(Arc::new(map))
    }
}

/// DTO Schema Definition
#[derive(Debug, Clone)]
pub struct Schema {
    pub name: String,
    pub mode: DTOMode,
    pub allow_coercion: bool,
    pub fields: Vec<FieldSpec>,
}

impl Schema {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            mode: DTOMode::Strict,
            allow_coercion: false,
            fields: Vec::new(),
        }
    }

    pub fn with_mode(mut self, mode: DTOMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_coercion(mut self, coerce: bool) -> Self {
        self.allow_coercion = coerce;
        self
    }

    pub fn add_field(&mut self, spec: FieldSpec) {
        self.fields.push(spec);
    }

    /// Validates an incoming untrusted Value against this DTO schema at runtime.
    pub fn validate(&self, val: &Value) -> Result<Value, ValidationError> {
        let mut field_errors: Vec<FieldError> = Vec::new();
        let mut path_stack: Vec<String> = Vec::new();

        let obj_map = match val {
            Value::Object(map_arc) => map_arc.clone(),
            _ => {
                field_errors.push(FieldError {
                    path: vec![],
                    code: "type_mismatch".to_string(),
                    message: format!(
                        "Expected object for DTO '{}', received non-object",
                        self.name
                    ),
                    expected: "Object".to_string(),
                    received: format!("{:?}", val),
                });
                return Err(ValidationError::new(
                    format!("Validation failed for DTO '{}'", self.name),
                    field_errors,
                ));
            }
        };

        // Strict mode check for unknown properties
        if self.mode == DTOMode::Strict {
            let obj_guard = obj_map.clone(); // Arc<FastMap>
            for key in obj_guard.keys() {
                if !self.fields.iter().any(|f| &f.name == key) {
                    field_errors.push(FieldError {
                        path: vec![key.clone()],
                        code: "unknown_property".to_string(),
                        message: format!(
                            "Unknown property '{}' is not declared in strict DTO '{}'",
                            key, self.name
                        ),
                        expected: "Declared fields only".to_string(),
                        received: key.clone(),
                    });
                }
            }
        }

        let mut validated_map = FastMap::default();

        for spec in &self.fields {
            path_stack.push(spec.name.clone());
            let raw_val_opt = obj_map.get(&spec.name).cloned();

            match (raw_val_opt, &spec.default_value) {
                (None, Some(def)) => {
                    validated_map.insert(spec.name.clone(), def.clone());
                }
                (None, None) => {
                    if spec.is_required {
                        field_errors.push(FieldError {
                            path: path_stack.clone(),
                            code: "required_missing".to_string(),
                            message: format!("Required field '{}' is missing", spec.name),
                            expected: format!("{:?}", spec.field_type),
                            received: "undefined".to_string(),
                        });
                    }
                }
                (Some(Value::Null), _) => {
                    if spec.is_nullable {
                        validated_map.insert(spec.name.clone(), Value::Null);
                    } else {
                        field_errors.push(FieldError {
                            path: path_stack.clone(),
                            code: "non_nullable".to_string(),
                            message: format!("Field '{}' cannot be null", spec.name),
                            expected: "Non-null value".to_string(),
                            received: "null".to_string(),
                        });
                    }
                }
                (Some(in_val), _) => {
                    match self.validate_field_type(
                        &spec.field_type,
                        &in_val,
                        self.allow_coercion,
                        &path_stack,
                    ) {
                        Ok(norm_val) => {
                            // Check constraints
                            for constraint in &spec.constraints {
                                if let Err(err_msg) = self.check_constraint(constraint, &norm_val) {
                                    field_errors.push(FieldError {
                                        path: path_stack.clone(),
                                        code: "constraint_violation".to_string(),
                                        message: err_msg,
                                        expected: format!("{:?}", constraint),
                                        received: format!("{:?}", norm_val),
                                    });
                                }
                            }
                            validated_map.insert(spec.name.clone(), norm_val);
                        }
                        Err(e) => {
                            field_errors.push(e);
                        }
                    }
                }
            }

            path_stack.pop();
        }

        if !field_errors.is_empty() {
            Err(ValidationError::new(
                format!("Validation failed for DTO '{}'", self.name),
                field_errors,
            ))
        } else {
            Ok(Value::Object(Arc::new(validated_map)))
        }
    }

    /// Serializes a DTO response object, stripping sensitive (@secret / @writeOnly) fields.
    pub fn serialize_response(&self, val: &Value) -> Result<Value, String> {
        let obj_map = match val {
            Value::Object(m) => m,
            _ => return Err("Response data is not an object".to_string()),
        };

        let mut output_map = FastMap::default();

        for spec in &self.fields {
            // Sensitive fields (@secret or @writeOnly) must never be serialized
            if spec.is_secret || spec.is_write_only {
                continue;
            }

            if let Some(v) = obj_map.get(&spec.name) {
                output_map.insert(spec.name.clone(), v.clone());
            } else if let Some(def) = &spec.default_value {
                output_map.insert(spec.name.clone(), def.clone());
            }
        }

        Ok(Value::Object(Arc::new(output_map)))
    }

    fn validate_field_type(
        &self,
        ftype: &FieldType,
        val: &Value,
        allow_coercion: bool,
        path: &[String],
    ) -> Result<Value, FieldError> {
        match (ftype, val) {
            (FieldType::String, Value::Str(s)) => Ok(Value::Str(s.clone())),
            (FieldType::String, other) if allow_coercion => Ok(Value::Str(format!("{:?}", other))),

            // Int Validation
            (FieldType::Int, Value::I64(i)) => Ok(Value::I64(*i)),
            (FieldType::Int, Value::U64(u)) => {
                if *u <= (i64::MAX as u64) {
                    Ok(Value::I64(*u as i64))
                } else {
                    Err(FieldError {
                        path: path.to_vec(),
                        code: "integer_overflow".to_string(),
                        message: format!("Unsigned integer {} exceeds maximum i64 bounds", u),
                        expected: "i64".to_string(),
                        received: u.to_string(),
                    })
                }
            }
            (FieldType::Int, Value::U32(u)) => Ok(Value::I64(*u as i64)),
            (FieldType::Int, Value::I32(i)) => Ok(Value::I64(*i as i64)),
            (FieldType::Int, Value::I16(i)) => Ok(Value::I64(*i as i64)),
            (FieldType::Int, Value::U16(u)) => Ok(Value::I64(*u as i64)),
            (FieldType::Int, Value::I8(i)) => Ok(Value::I64(*i as i64)),
            (FieldType::Int, Value::U8(u)) => Ok(Value::I64(*u as i64)),
            (FieldType::Int, Value::Number(n)) => {
                if n.fract() != 0.0 {
                    Err(FieldError {
                        path: path.to_vec(),
                        code: "fractional_rejected".to_string(),
                        message: format!(
                            "Fractional value {} is not a valid integer for Int DTO field",
                            n
                        ),
                        expected: "Integer".to_string(),
                        received: n.to_string(),
                    })
                } else {
                    Ok(Value::I64(*n as i64))
                }
            }
            (FieldType::Int, Value::Str(s)) if allow_coercion => s
                .trim()
                .parse::<i64>()
                .map(Value::I64)
                .map_err(|_| FieldError {
                    path: path.to_vec(),
                    code: "invalid_coercion".to_string(),
                    message: format!("Cannot coerce string '{}' to integer", s),
                    expected: "Integer".to_string(),
                    received: s.clone(),
                }),

            // U32 Validation
            (FieldType::U32, Value::U32(u)) => Ok(Value::U32(*u)),
            (FieldType::U32, Value::U8(u)) => Ok(Value::U32(*u as u32)),
            (FieldType::U32, Value::U16(u)) => Ok(Value::U32(*u as u32)),
            (FieldType::U32, Value::I64(i)) => {
                if *i >= 0 && *i <= (u32::MAX as i64) {
                    Ok(Value::U32(*i as u32))
                } else {
                    Err(FieldError {
                        path: path.to_vec(),
                        code: "out_of_bounds".to_string(),
                        message: format!("Integer {} is out of u32 bounds [0, {}]", i, u32::MAX),
                        expected: "u32".to_string(),
                        received: i.to_string(),
                    })
                }
            }
            (FieldType::U32, Value::U64(u)) => {
                if *u <= (u32::MAX as u64) {
                    Ok(Value::U32(*u as u32))
                } else {
                    Err(FieldError {
                        path: path.to_vec(),
                        code: "out_of_bounds".to_string(),
                        message: format!("Integer {} is out of u32 bounds [0, {}]", u, u32::MAX),
                        expected: "u32".to_string(),
                        received: u.to_string(),
                    })
                }
            }
            (FieldType::U32, Value::Number(n)) => {
                if n.fract() != 0.0 {
                    Err(FieldError {
                        path: path.to_vec(),
                        code: "fractional_rejected".to_string(),
                        message: format!(
                            "Fractional value {} is not a valid integer for u32 DTO field",
                            n
                        ),
                        expected: "u32".to_string(),
                        received: n.to_string(),
                    })
                } else if *n >= 0.0 && *n <= (u32::MAX as f64) {
                    Ok(Value::U32(*n as u32))
                } else {
                    Err(FieldError {
                        path: path.to_vec(),
                        code: "out_of_bounds".to_string(),
                        message: format!("Number {} is out of u32 bounds [0, {}]", n, u32::MAX),
                        expected: "u32".to_string(),
                        received: n.to_string(),
                    })
                }
            }
            (FieldType::U32, Value::Str(s)) if allow_coercion => s
                .trim()
                .parse::<u32>()
                .map(Value::U32)
                .map_err(|_| FieldError {
                    path: path.to_vec(),
                    code: "invalid_coercion".to_string(),
                    message: format!("Cannot coerce string '{}' to u32", s),
                    expected: "u32".to_string(),
                    received: s.clone(),
                }),

            // Float Validation
            (FieldType::Float, Value::Number(n)) => Ok(Value::Number(*n)),
            (FieldType::Float, Value::I64(i)) => Ok(Value::Number(*i as f64)),
            (FieldType::Float, Value::U64(u)) => Ok(Value::Number(*u as f64)),
            (FieldType::Float, Value::I32(i)) => Ok(Value::Number(*i as f64)),
            (FieldType::Float, Value::U32(u)) => Ok(Value::Number(*u as f64)),
            (FieldType::Float, Value::I16(i)) => Ok(Value::Number(*i as f64)),
            (FieldType::Float, Value::U16(u)) => Ok(Value::Number(*u as f64)),
            (FieldType::Float, Value::I8(i)) => Ok(Value::Number(*i as f64)),
            (FieldType::Float, Value::U8(u)) => Ok(Value::Number(*u as f64)),

            // Bool Validation
            (FieldType::Bool, Value::Bool(b)) => Ok(Value::Bool(*b)),
            (FieldType::Bool, Value::Str(s)) if allow_coercion => {
                match s.trim().to_lowercase().as_str() {
                    "true" | "1" => Ok(Value::Bool(true)),
                    "false" | "0" => Ok(Value::Bool(false)),
                    _ => Err(FieldError {
                        path: path.to_vec(),
                        code: "invalid_coercion".to_string(),
                        message: format!("Cannot coerce string '{}' to boolean", s),
                        expected: "boolean".to_string(),
                        received: s.clone(),
                    }),
                }
            }

            (FieldType::Domain(kind), Value::Str(s)) => kind
                .validate_str(s)
                .map(|_| Value::Str(s.clone()))
                .map_err(|e| FieldError {
                    path: path.to_vec(),
                    code: "domain_validation_error".to_string(),
                    message: e,
                    expected: kind.name().to_string(),
                    received: s.clone(),
                }),

            (FieldType::Enum(variants), Value::Str(s)) => {
                if variants.contains(s) {
                    Ok(Value::Str(s.clone()))
                } else {
                    Err(FieldError {
                        path: path.to_vec(),
                        code: "invalid_enum_variant".to_string(),
                        message: format!(
                            "Value '{}' is not a valid variant of enum {:?}",
                            s, variants
                        ),
                        expected: format!("{:?}", variants),
                        received: s.clone(),
                    })
                }
            }

            (FieldType::Array(inner_t), Value::Array(arr)) => {
                let mut norm_arr = Vec::new();
                for (idx, elem) in arr.iter().enumerate() {
                    let mut elem_path = path.to_vec();
                    elem_path.push(idx.to_string());
                    let norm_elem =
                        self.validate_field_type(inner_t, elem, allow_coercion, &elem_path)?;
                    norm_arr.push(norm_elem);
                }
                Ok(Value::Array(norm_arr))
            }

            (FieldType::Object(schema), obj_val) => {
                schema.validate(obj_val).map_err(|e| FieldError {
                    path: path.to_vec(),
                    code: "nested_validation_error".to_string(),
                    message: e.message,
                    expected: schema.name.clone(),
                    received: format!("{:?}", obj_val),
                })
            }

            (FieldType::Any, val) => Ok(val.clone()),

            _ => Err(FieldError {
                path: path.to_vec(),
                code: "type_mismatch".to_string(),
                message: format!("Type mismatch: expected {:?}, found {:?}", ftype, val),
                expected: format!("{:?}", ftype),
                received: format!("{:?}", val),
            }),
        }
    }

    fn check_constraint(&self, constraint: &Constraint, val: &Value) -> Result<(), String> {
        match (constraint, val) {
            (Constraint::Min(min_val), Value::I64(i)) => {
                if (*i as f64) < *min_val {
                    Err(format!(
                        "Value {} is less than minimum allowed ({})",
                        i, min_val
                    ))
                } else {
                    Ok(())
                }
            }
            (Constraint::Min(min_val), Value::U32(u)) => {
                if (*u as f64) < *min_val {
                    Err(format!(
                        "Value {} is less than minimum allowed ({})",
                        u, min_val
                    ))
                } else {
                    Ok(())
                }
            }
            (Constraint::Min(min_val), Value::U64(u)) => {
                if (*u as f64) < *min_val {
                    Err(format!(
                        "Value {} is less than minimum allowed ({})",
                        u, min_val
                    ))
                } else {
                    Ok(())
                }
            }
            (Constraint::Min(min_val), Value::Number(n)) => {
                if *n < *min_val {
                    Err(format!(
                        "Value {} is less than minimum allowed ({})",
                        n, min_val
                    ))
                } else {
                    Ok(())
                }
            }
            (Constraint::Max(max_val), Value::I64(i)) => {
                if (*i as f64) > *max_val {
                    Err(format!(
                        "Value {} is greater than maximum allowed ({})",
                        i, max_val
                    ))
                } else {
                    Ok(())
                }
            }
            (Constraint::Max(max_val), Value::U32(u)) => {
                if (*u as f64) > *max_val {
                    Err(format!(
                        "Value {} is greater than maximum allowed ({})",
                        u, max_val
                    ))
                } else {
                    Ok(())
                }
            }
            (Constraint::Max(max_val), Value::U64(u)) => {
                if (*u as f64) > *max_val {
                    Err(format!(
                        "Value {} is greater than maximum allowed ({})",
                        u, max_val
                    ))
                } else {
                    Ok(())
                }
            }
            (Constraint::Max(max_val), Value::Number(n)) => {
                if *n > *max_val {
                    Err(format!(
                        "Value {} is greater than maximum allowed ({})",
                        n, max_val
                    ))
                } else {
                    Ok(())
                }
            }
            (Constraint::MinLength(min_len), Value::Str(s)) => {
                if s.len() < *min_len {
                    Err(format!(
                        "String length {} is less than minimum length ({})",
                        s.len(),
                        min_len
                    ))
                } else {
                    Ok(())
                }
            }
            (Constraint::MaxLength(max_len), Value::Str(s)) => {
                if s.len() > *max_len {
                    Err(format!(
                        "String length {} exceeds maximum length ({})",
                        s.len(),
                        max_len
                    ))
                } else {
                    Ok(())
                }
            }
            (Constraint::ExactLength(exact_len), Value::Str(s)) => {
                if s.len() != *exact_len {
                    Err(format!(
                        "String length {} does not match exact required length ({})",
                        s.len(),
                        exact_len
                    ))
                } else {
                    Ok(())
                }
            }
            (Constraint::Pattern(pattern), Value::Str(s)) => match regex::Regex::new(pattern) {
                Ok(re) => {
                    if re.is_match(s) {
                        Ok(())
                    } else {
                        Err(format!(
                            "String '{}' does not match required regex pattern '{}'",
                            s, pattern
                        ))
                    }
                }
                Err(_) => Err(format!("Invalid regex pattern rule '{}'", pattern)),
            },
            (Constraint::Domain(kind), Value::Str(s)) => kind.validate_str(s),
            _ => Ok(()),
        }
    }
}
