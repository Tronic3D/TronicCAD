//! The TronicCAD cloud service as a provider. See `services/cloud`.

use serde_json::Value;

use crate::http::{agent, checked_base, read_body, status_error, unreachable};
use crate::provider::{check_key, StorageError, StorageProvider};

pub struct CloudProvider {
    base: String,
    token: String,
    agent: ureq::Agent,
}

impl CloudProvider {
    pub fn new(base: &str, token: &str) -> Result<Self, StorageError> {
        let (base, local) = checked_base(base)?;
        Ok(Self {
            base,
            token: token.to_owned(),
            agent: agent(local),
        })
    }

    fn post(&self, path: &str, body: &Value) -> Result<Value, StorageError> {
        let mut response = self
            .agent
            .post(format!("{}{path}", self.base))
            .header("authorization", format!("Bearer {}", self.token))
            .header("content-type", "application/json")
            .send(body.to_string())
            .map_err(|e| unreachable(&e))?;
        let status = response.status().as_u16();
        let bytes = read_body(&mut response)?;
        if !(200..300).contains(&status) {
            return Err(status_error(status, &bytes));
        }
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|_| StorageError::BadResponse)
    }

    /// Any other request of the service, for what is not about objects.
    pub fn call(&self, method: &str, path: &str, body: &Value) -> Result<Value, StorageError> {
        let request = ureq::http::Request::builder()
            .method(method)
            .uri(format!("{}{path}", self.base))
            .header("authorization", format!("Bearer {}", self.token))
            .header("content-type", "application/json")
            .body(body.to_string())
            .map_err(|_| StorageError::InvalidUrl)?;
        let mut response = self.agent.run(request).map_err(|e| unreachable(&e))?;
        let status = response.status().as_u16();
        let bytes = read_body(&mut response)?;
        if !(200..300).contains(&status) {
            return Err(status_error(status, &bytes));
        }
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|_| StorageError::BadResponse)
    }

    /// Creates an account. Returns a login token.
    pub fn register(base: &str, email: &str, password: &str) -> Result<String, StorageError> {
        Self::register_invited(base, email, password, None)
    }

    /// The same with an invitation, which a server with closed registration asks for.
    pub fn register_invited(
        base: &str,
        email: &str,
        password: &str,
        invitation: Option<&str>,
    ) -> Result<String, StorageError> {
        let client = Self::new(base, "")?;
        let answer = client.post(
            "/v1/auth/register",
            &serde_json::json!({"email": email, "password": password, "invitation": invitation}),
        )?;
        answer["token"]
            .as_str()
            .map(str::to_owned)
            .ok_or(StorageError::BadResponse)
    }

    /// Logs in. Returns a login token; the password itself is not kept.
    pub fn login(base: &str, email: &str, password: &str) -> Result<String, StorageError> {
        let client = Self::new(base, "")?;
        let answer = client.post(
            "/v1/auth/login",
            &serde_json::json!({"email": email, "password": password}),
        )?;
        answer["token"]
            .as_str()
            .map(str::to_owned)
            .ok_or(StorageError::BadResponse)
    }

    pub fn logout(&self) -> Result<(), StorageError> {
        self.post("/v1/auth/logout", &Value::Null).map(|_| ())
    }

    pub fn account(&self) -> Result<Value, StorageError> {
        let mut response = self
            .agent
            .get(format!("{}/v1/account", self.base))
            .header("authorization", format!("Bearer {}", self.token))
            .call()
            .map_err(|e| unreachable(&e))?;
        let status = response.status().as_u16();
        let bytes = read_body(&mut response)?;
        if !(200..300).contains(&status) {
            return Err(status_error(status, &bytes));
        }
        serde_json::from_slice(&bytes).map_err(|_| StorageError::BadResponse)
    }
}

impl StorageProvider for CloudProvider {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError> {
        check_key(key)?;
        let mut response = self
            .agent
            .put(format!("{}/v1/objects/{key}", self.base))
            .header("authorization", format!("Bearer {}", self.token))
            .header("content-type", "application/octet-stream")
            .send(bytes)
            .map_err(|e| unreachable(&e))?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        }
        let body = read_body(&mut response).unwrap_or_default();
        Err(status_error(status, &body))
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        check_key(key)?;
        let mut response = self
            .agent
            .get(format!("{}/v1/objects/{key}", self.base))
            .header("authorization", format!("Bearer {}", self.token))
            .call()
            .map_err(|e| unreachable(&e))?;
        let status = response.status().as_u16();
        if status == 404 {
            return Ok(None);
        }
        let body = read_body(&mut response)?;
        if !(200..300).contains(&status) {
            return Err(status_error(status, &body));
        }
        Ok(Some(body))
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let prefix = prefix.trim_end_matches('/');
        check_key(prefix)?;
        let mut response = self
            .agent
            .get(format!("{}/v1/list/{prefix}", self.base))
            .header("authorization", format!("Bearer {}", self.token))
            .call()
            .map_err(|e| unreachable(&e))?;
        let status = response.status().as_u16();
        let body = read_body(&mut response)?;
        if !(200..300).contains(&status) {
            return Err(status_error(status, &body));
        }
        let value: Value = serde_json::from_slice(&body).map_err(|_| StorageError::BadResponse)?;
        let keys = value["keys"].as_array().ok_or(StorageError::BadResponse)?;
        Ok(keys
            .iter()
            .filter_map(|key| key.as_str())
            .filter(|key| check_key(key).is_ok())
            .map(str::to_owned)
            .collect())
    }
}
