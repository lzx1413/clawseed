//! Pairing guard for gateway authentication.

use parking_lot::Mutex;

/// Guard that manages pairing state for the gateway.
pub struct PairingGuard {
    inner: Mutex<PairingInner>,
}

struct PairingInner {
    require_pairing: bool,
    paired_tokens: Vec<String>,
    pairing_code: Option<String>,
}

impl PairingGuard {
    /// Create a new pairing guard.
    pub fn new(require_pairing: bool, paired_tokens: &[String]) -> Self {
        let pairing_code = if require_pairing && paired_tokens.is_empty() {
            Some(generate_pairing_code())
        } else {
            None
        };

        Self {
            inner: Mutex::new(PairingInner {
                require_pairing,
                paired_tokens: paired_tokens.to_vec(),
                pairing_code,
            }),
        }
    }

    /// Whether pairing is required for access.
    pub fn require_pairing(&self) -> bool {
        self.inner.lock().require_pairing
    }

    /// Whether at least one device has been paired.
    pub fn is_paired(&self) -> bool {
        !self.inner.lock().paired_tokens.is_empty()
    }

    /// Check if a bearer token is valid.
    pub fn is_authenticated(&self, token: &str) -> bool {
        if token.is_empty() {
            return false;
        }
        let inner = self.inner.lock();
        inner
            .paired_tokens
            .iter()
            .any(|t| constant_time_eq(t, token))
    }

    /// Get the current pairing code, if any.
    pub fn pairing_code(&self) -> Option<String> {
        self.inner.lock().pairing_code.clone()
    }

    /// Try to pair with a code. Returns the new bearer token on success.
    pub async fn try_pair(&self, code: &str, _client_key: &str) -> Result<Option<String>, u64> {
        let mut inner = self.inner.lock();
        if let Some(ref expected) = inner.pairing_code {
            if constant_time_eq(expected, code) {
                let token = generate_bearer_token();
                inner.paired_tokens.push(token.clone());
                inner.pairing_code = None;
                return Ok(Some(token));
            }
        }
        Err(0)
    }

    /// Generate a new pairing code.
    pub fn generate_new_pairing_code(&self) -> Option<String> {
        let mut inner = self.inner.lock();
        if inner.require_pairing {
            let code = generate_pairing_code();
            inner.pairing_code = Some(code.clone());
            Some(code)
        } else {
            None
        }
    }

    /// Get all paired tokens.
    pub fn tokens(&self) -> Vec<String> {
        self.inner.lock().paired_tokens.clone()
    }
}

/// Constant-time comparison to prevent timing attacks.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

/// Check if a bind address is public (not loopback or localhost).
pub fn is_public_bind(host: &str) -> bool {
    !matches!(host, "127.0.0.1" | "localhost" | "::1" | "0.0.0.0")
}

fn generate_pairing_code() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    format!(
        "{:04}-{:04}-{:04}",
        rng.random_range(0u16..10000),
        rng.random_range(0u16..10000),
        rng.random_range(0u16..10000),
    )
}

fn generate_bearer_token() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes);
    hex::encode(bytes)
}
