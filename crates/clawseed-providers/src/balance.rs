//! Optional account balances. Billing endpoints are provider-specific and are
//! deliberately separate from inference usage and model capabilities.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BalanceAmount {
    pub currency: String,
    /// Decimal string, preserving the provider's precision.
    pub available: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BalanceStatus {
    Available,
    Unsupported,
    MissingKey,
    AuthenticationFailed,
    PermissionDenied,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBalance {
    pub status: BalanceStatus,
    pub balances: Vec<BalanceAmount>,
}

impl ProviderBalance {
    pub fn status(status: BalanceStatus) -> Self {
        Self {
            status,
            balances: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum BalanceApi {
    DeepSeek,
    Moonshot,
    OpenRouter,
}

fn endpoint(base_url: &str) -> Option<(BalanceApi, &'static str)> {
    let url = reqwest::Url::parse(base_url).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    match (url.host_str()?, url.path().trim_end_matches('/')) {
        ("api.deepseek.com", "" | "/v1" | "/anthropic") => Some((
            BalanceApi::DeepSeek,
            "https://api.deepseek.com/user/balance",
        )),
        ("api.moonshot.cn", "" | "/v1") => Some((
            BalanceApi::Moonshot,
            "https://api.moonshot.cn/v1/users/me/balance",
        )),
        ("openrouter.ai", "/api/v1") => Some((
            BalanceApi::OpenRouter,
            "https://openrouter.ai/api/v1/credits",
        )),
        _ => None,
    }
}

fn decimal(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    text.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .map(|_| text)
}

fn parse_balance(api: BalanceApi, value: Value) -> Option<Vec<BalanceAmount>> {
    match api {
        BalanceApi::DeepSeek => {
            let rows = value.get("balance_infos")?.as_array()?;
            if rows.is_empty() {
                return None;
            }
            rows.iter()
                .map(|row| {
                    Some(BalanceAmount {
                        currency: row.get("currency")?.as_str()?.to_owned(),
                        available: decimal(row.get("total_balance")?)?,
                    })
                })
                .collect()
        }
        BalanceApi::Moonshot => {
            if !value.get("status")?.as_bool()? {
                return None;
            }
            Some(vec![BalanceAmount {
                currency: "CNY".into(),
                available: decimal(value.pointer("/data/available_balance")?)?,
            }])
        }
        BalanceApi::OpenRouter => {
            let credits = value.pointer("/data/total_credits")?.as_f64()?;
            let used = value.pointer("/data/total_usage")?.as_f64()?;
            let remaining = credits - used;
            if !remaining.is_finite() {
                return None;
            }
            Some(vec![BalanceAmount {
                currency: "USD".into(),
                available: format!("{remaining:.8}"),
            }])
        }
    }
}

/// Only send the supplied credential to a recognized official billing host.
/// Unsupported services cause no HTTP request. Errors never expose keys or bodies.
pub async fn query_balance(base_url: &str, api_key: Option<&str>) -> ProviderBalance {
    let Some((api, url)) = endpoint(base_url) else {
        return ProviderBalance::status(BalanceStatus::Unsupported);
    };
    let Some(key) = api_key.map(str::trim).filter(|key| !key.is_empty()) else {
        return ProviderBalance::status(BalanceStatus::MissingKey);
    };
    let client = clawseed_config::schema::build_runtime_proxy_client_with_timeouts(
        "provider.balance",
        15,
        10,
    );
    let response = match client.get(url).bearer_auth(key).send().await {
        Ok(response) => response,
        Err(_) => return ProviderBalance::status(BalanceStatus::Unavailable),
    };
    if !response.status().is_success() {
        return ProviderBalance::status(match response.status().as_u16() {
            401 => BalanceStatus::AuthenticationFailed,
            403 => BalanceStatus::PermissionDenied,
            _ => BalanceStatus::Unavailable,
        });
    }
    match response
        .json::<Value>()
        .await
        .ok()
        .and_then(|value| parse_balance(api, value))
    {
        Some(balances) => ProviderBalance {
            status: BalanceStatus::Available,
            balances,
        },
        None => ProviderBalance::status(BalanceStatus::Unavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn official_endpoint_matching_does_not_leak_keys_to_lookalikes() {
        assert_eq!(
            endpoint("https://api.deepseek.com/v1/").unwrap().0,
            BalanceApi::DeepSeek
        );
        for url in [
            "http://api.deepseek.com/v1",
            "https://api.deepseek.com.evil.test/v1",
            "https://api.deepseek.com@evil.test/v1",
            "https://api.deepseek.com/custom",
            "https://api.deepseek.com:8080/v1",
            "https://api.deepseek.com/v1?key=x",
            "https://api.openai.com/v1",
        ] {
            assert!(endpoint(url).is_none(), "{url}");
        }
    }

    #[test]
    fn preserves_zero_negative_and_multi_currency_balances() {
        let balances = parse_balance(
            BalanceApi::DeepSeek,
            json!({"balance_infos": [
                {"currency": "CNY", "total_balance": "0.00"},
                {"currency": "USD", "total_balance": "-0.00123"}
            ]}),
        )
        .unwrap();
        assert_eq!(balances[0].available, "0.00");
        assert_eq!(balances[1].available, "-0.00123");
        assert!(parse_balance(BalanceApi::DeepSeek, json!({})).is_none());
        assert!(parse_balance(BalanceApi::DeepSeek, json!({"balance_infos": []})).is_none());
        assert_eq!(
            parse_balance(
                BalanceApi::Moonshot,
                json!({"status": true, "data": {"available_balance": 49.58894}})
            )
            .unwrap()[0]
                .available,
            "49.58894"
        );
        assert!(
            parse_balance(
                BalanceApi::Moonshot,
                json!({"status": false, "data": {"available_balance": 0}})
            )
            .is_none()
        );
        assert_eq!(
            parse_balance(
                BalanceApi::OpenRouter,
                json!({"data": {"total_credits": 100.5, "total_usage": 25.75}})
            )
            .unwrap()[0]
                .available,
            "74.75000000"
        );
    }

    #[tokio::test]
    async fn unsupported_and_missing_key_require_no_network() {
        assert_eq!(
            query_balance("https://api.openai.com/v1", Some("secret"))
                .await
                .status,
            BalanceStatus::Unsupported
        );
        assert_eq!(
            query_balance("https://api.deepseek.com/v1", None)
                .await
                .status,
            BalanceStatus::MissingKey
        );
    }
}
