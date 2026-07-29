use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct DynamicToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl Serialize for DynamicToolDefinition {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        json!({
            "type": "function",
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        })
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DynamicToolDefinition {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        if value.get("type").and_then(Value::as_str) != Some("function") {
            return Err(D::Error::custom(
                "dynamic tool definition must have type function",
            ));
        }
        Ok(Self {
            name: value
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| D::Error::custom("dynamic tool definition requires name"))?
                .to_owned(),
            description: value
                .get("description")
                .and_then(Value::as_str)
                .ok_or_else(|| D::Error::custom("dynamic tool definition requires description"))?
                .to_owned(),
            input_schema: value
                .get("inputSchema")
                .cloned()
                .ok_or_else(|| D::Error::custom("dynamic tool definition requires inputSchema"))?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolCall {
    pub thread_id: String,
    pub turn_id: String,
    pub call_id: String,
    pub tool: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DynamicToolOutput {
    pub success: bool,
    pub content_items: Vec<Value>,
}

impl DynamicToolOutput {
    pub fn success(values: Vec<Value>) -> Result<Self> {
        let content_items = values
            .into_iter()
            .map(|value| {
                Ok(json!({
                    "type": "inputText",
                    "text": serde_json::to_string(&value)?,
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            success: true,
            content_items,
        })
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            content_items: vec![json!({
                "type": "inputText",
                "text": json!({"error": message.into()}).to_string(),
            })],
        }
    }
}

#[async_trait]
pub trait DynamicToolHandler: Send + Sync {
    async fn call(&self, call: DynamicToolCall) -> Result<Vec<Value>>;
}

pub struct DynamicToolSession {
    pub definitions: Vec<DynamicToolDefinition>,
    pub handler: Arc<dyn DynamicToolHandler>,
}

impl DynamicToolSession {
    pub fn validate(&self) -> Result<()> {
        if self.definitions.is_empty() {
            bail!("dynamic tool session requires at least one definition");
        }
        for definition in &self.definitions {
            if definition.name.trim().is_empty() {
                bail!("dynamic tool name cannot be empty");
            }
            if definition.description.trim().is_empty() {
                bail!("dynamic tool description cannot be empty");
            }
        }
        Ok(())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.definitions
            .iter()
            .any(|definition| definition.name == name)
    }
}
