//! Test double for the herdr socket API.

use std::collections::HashMap;
use std::sync::Mutex;

use herdr_client::{Api, Error};
use serde_json::Value;

#[derive(Default)]
pub struct FakeApi {
    responses: HashMap<String, Result<Value, (String, String)>>,
    calls: Mutex<Vec<(String, Value)>>,
}

impl FakeApi {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ok(mut self, method: &str, result: Value) -> Self {
        self.responses.insert(method.to_string(), Ok(result));
        self
    }

    pub fn err(mut self, method: &str, code: &str, message: &str) -> Self {
        self.responses.insert(method.to_string(), Err((code.to_string(), message.to_string())));
        self
    }

    pub fn calls(&self) -> Vec<(String, Value)> {
        self.calls.lock().unwrap().clone()
    }
}

impl Api for FakeApi {
    fn request(&self, method: &str, params: Value) -> Result<Value, Error> {
        self.calls.lock().unwrap().push((method.to_string(), params));
        match self.responses.get(method) {
            Some(Ok(value)) => Ok(value.clone()),
            Some(Err((code, message))) => {
                Err(Error::Api { method: method.to_string(), code: code.clone(), message: message.clone() })
            }
            None => Err(Error::Api {
                method: method.to_string(),
                code: "unknown_method".into(),
                message: "not faked".into(),
            }),
        }
    }
}
