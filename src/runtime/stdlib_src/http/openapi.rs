//! OpenAPI 3.0.3 & JSON Schema Document Generator for AdeshLang HTTP Standard Library.

use crate::parsing::ast::Value;
use crate::runtime::stdlib_src::http::schema::{FieldType, Schema};
use crate::utils::collections::FastMap;
use std::sync::Arc;

/// OpenAPI Document Generator
pub struct OpenAPIGenerator;

impl OpenAPIGenerator {
    /// Converts an AdeshLang DTO Schema to OpenAPI 3.0.3 / JSON Schema Object
    pub fn schema_to_json_schema(schema: &Schema) -> Value {
        let mut map = FastMap::default();
        map.insert("type".to_string(), Value::Str("object".to_string()));
        map.insert("title".to_string(), Value::Str(schema.name.clone()));

        let mut props_map = FastMap::default();
        let mut required_list: Vec<Value> = Vec::new();

        for spec in &schema.fields {
            if spec.is_secret || spec.is_write_only {
                // Secret fields omitted from public schemas
                continue;
            }

            if spec.is_required {
                required_list.push(Value::Str(spec.name.clone()));
            }

            let mut field_schema = FastMap::default();
            match &spec.field_type {
                FieldType::String => {
                    field_schema.insert("type".to_string(), Value::Str("string".to_string()));
                }
                FieldType::Int | FieldType::U32 => {
                    field_schema.insert("type".to_string(), Value::Str("integer".to_string()));
                }
                FieldType::Float => {
                    field_schema.insert("type".to_string(), Value::Str("number".to_string()));
                }
                FieldType::Bool => {
                    field_schema.insert("type".to_string(), Value::Str("boolean".to_string()));
                }
                FieldType::Domain(kind) => {
                    field_schema.insert("type".to_string(), Value::Str("string".to_string()));
                    field_schema
                        .insert("format".to_string(), Value::Str(kind.name().to_lowercase()));
                }
                FieldType::Enum(variants) => {
                    field_schema.insert("type".to_string(), Value::Str("string".to_string()));
                    let enum_arr: Vec<Value> =
                        variants.iter().map(|v| Value::Str(v.clone())).collect();
                    field_schema.insert("enum".to_string(), Value::Array(enum_arr));
                }
                FieldType::Array(inner) => {
                    field_schema.insert("type".to_string(), Value::Str("array".to_string()));
                    let mut inner_map = FastMap::default();
                    inner_map.insert("type".to_string(), Value::Str(format!("{:?}", inner)));
                    field_schema.insert("items".to_string(), Value::Object(Arc::new(inner_map)));
                }
                FieldType::Object(sub_schema) => {
                    return Self::schema_to_json_schema(sub_schema);
                }
                FieldType::Map(_) | FieldType::Any => {
                    field_schema.insert("type".to_string(), Value::Str("object".to_string()));
                }
            }

            props_map.insert(spec.name.clone(), Value::Object(Arc::new(field_schema)));
        }

        map.insert("properties".to_string(), Value::Object(Arc::new(props_map)));
        if !required_list.is_empty() {
            map.insert("required".to_string(), Value::Array(required_list));
        }

        Value::Object(Arc::new(map))
    }

    /// Generates a complete OpenAPI 3.0.3 specification root object
    pub fn generate_openapi_spec(title: &str, version: &str, schemas: Vec<&Schema>) -> Value {
        let mut root = FastMap::default();
        root.insert("openapi".to_string(), Value::Str("3.0.3".to_string()));

        let mut info = FastMap::default();
        info.insert("title".to_string(), Value::Str(title.to_string()));
        info.insert("version".to_string(), Value::Str(version.to_string()));
        root.insert("info".to_string(), Value::Object(Arc::new(info)));

        let mut components = FastMap::default();
        let mut schemas_map = FastMap::default();

        for s in schemas {
            schemas_map.insert(s.name.clone(), Self::schema_to_json_schema(s));
        }

        components.insert("schemas".to_string(), Value::Object(Arc::new(schemas_map)));
        root.insert(
            "components".to_string(),
            Value::Object(Arc::new(components)),
        );

        Value::Object(Arc::new(root))
    }
}
