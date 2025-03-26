use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use language_tags::LanguageTag;
use lib::utils::normal2stream;
#[cfg(test)]
use lib::utils::{test_translate, test_translate_stream};
use lib::{TranslateResult, TranslateStreamChunk, TranslateTask, Translator};
use reqwest::Request;
use reqwest::{Client, IntoUrl, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cmp::{min, Ordering};
use std::fmt::{Display, Formatter};
use tokio::sync::mpsc::Sender;

#[derive(Serialize, Deserialize, Debug, Clone)]
enum RequestMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
}

impl Display for RequestMethod {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestMethod::GET => {
                write!(f, "GET")
            }
            RequestMethod::POST => {
                write!(f, "POST")
            }
            RequestMethod::PUT => {
                write!(f, "PUT")
            }
            RequestMethod::DELETE => {
                write!(f, "DELETE")
            }
            RequestMethod::PATCH => {
                write!(f, "PATCH")
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TencentCredential {
    pub secret_id: String,
    pub secret_key: String,
    pub token: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TencentCloudRequest {
    pub host: String,
    pub method: RequestMethod,
    pub action: String,
    pub region: Option<String>,
    pub version: String,
    pub language: Option<String>,
    pub credential: TencentCredential,
    pub query: Option<Value>,
    pub body: Option<Value>,
}

impl TencentCloudRequest {
    pub fn build_request(&self, client: &reqwest::Client) -> Result<Request> {
        let mut builder = self
            .method
            .new_request(client, format!("https://{}", self.host));

        builder = builder.header("Host", self.host.clone());
        builder = builder.header("X-TC-Action", self.action.clone());
        builder = builder.header("X-TC-Version", self.version.clone());

        if let Some(region) = &self.region {
            builder = builder.header("X-TC-Region", region);
        }

        if let Some(language) = &self.language {
            builder = builder.header("X-TC-Language", language.clone());
        }

        if let Some(token) = &self.credential.token {
            builder = builder.header("X-TC-Token", token.clone());
        }

        if let Some(body) = &self.body {
            builder = builder.json(body);
        }

        if let Some(query) = &self.query {
            builder = builder.query(query);
        }

        let mut request = builder.build()?;

        self.sign(&mut request)?;

        Ok(request)
    }

    fn sign(&self, request: &mut Request) -> Result<()> {
        let now = chrono::Utc::now();
        let timestamp = now.timestamp();

        request
            .headers_mut()
            .insert("X-TC-Timestamp", timestamp.to_string().parse()?);

        let mut headers = request
            .headers()
            .iter()
            // .filter(|(a,b)| !a.to_string().starts_with("x-tc-") || a.to_string().to_lowercase() == "x-tc-action")
            .map(|(k, v)| {
                (
                    k.to_string().to_lowercase().trim().to_string(),
                    v.to_str().unwrap().to_lowercase().trim().to_string(),
                )
            })
            .collect::<Vec<_>>();

        headers.sort_by(|a, b| {
            let cmp = a.0.cmp(&b.0);

            if matches!(cmp, Ordering::Equal) {
                a.1.cmp(&b.1)
            } else {
                cmp
            }
        });

        let header_list = {
            let mut header_list = vec![];
            for (k, v) in headers.iter() {
                header_list.push(format!("{}:{}\n", k, v));
            }
            header_list.join("")
        };

        let signed_headers = headers
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>()
            .join(";");

        let canonical_request = {
            let mut canonical_requests = vec![];

            let canonical_uri = "/";
            let canonical_query_string = request.url().query().unwrap_or("");

            canonical_requests.push(self.method.to_string());
            canonical_requests.push(canonical_uri.to_string());
            canonical_requests.push(canonical_query_string.to_string());

            canonical_requests.push(header_list);

            canonical_requests.push(signed_headers.clone());

            if let Some(body) = request.body() {
                let digest = Sha256::new()
                    .chain_update(body.as_bytes().unwrap())
                    .finalize();
                canonical_requests.push(hex::encode(digest));
            }

            canonical_requests.join("\n")
        };

        // println!("{}", canonical_request);
        //
        // println!("==========");

        let hashed_canonical_request = {
            let digest = Sha256::new()
                .chain_update(canonical_request.as_bytes())
                .finalize();
            hex::encode(digest)
        };

        // println!("hashed_canonical_request = {}", hashed_canonical_request);
        //
        // println!("==========");

        let date = now.format("%Y-%m-%d").to_string();

        let service = self.host.split(".").nth(0).unwrap();

        let credential_scope = format!("{}/{}/tc3_request", date, service);

        let string_to_sign = {
            let mut string_to_sign_vec = vec![];

            string_to_sign_vec.push("TC3-HMAC-SHA256".to_string());

            string_to_sign_vec.push(timestamp.to_string());

            string_to_sign_vec.push(credential_scope.clone());

            string_to_sign_vec.push(hashed_canonical_request);

            string_to_sign_vec.join("\n")
        };

        // println!("{}", string_to_sign);
        //
        // println!("==========");

        let secret_key = self.credential.secret_key.clone();

        let secret_date = {
            let mut hmac =
                Hmac::<Sha256>::new_from_slice(format!("TC3{}", secret_key).as_bytes()).unwrap();
            hmac.update(date.as_bytes());
            hmac.finalize()
        };

        let secret_service = {
            let mut hmac =
                Hmac::<Sha256>::new_from_slice(secret_date.into_bytes().as_slice()).unwrap();
            hmac.update(service.as_bytes());
            hmac.finalize()
        };

        let secret_signing = {
            let mut hmac =
                Hmac::<Sha256>::new_from_slice(secret_service.into_bytes().as_slice()).unwrap();
            hmac.update("tc3_request".as_bytes());
            hmac.finalize()
        };

        let signing = {
            let mut hmac =
                Hmac::<Sha256>::new_from_slice(secret_signing.into_bytes().as_slice()).unwrap();
            hmac.update(string_to_sign.as_bytes());
            hmac.finalize()
        };

        let signature = hex::encode(signing.into_bytes());

        // println!("signature = {}", signature);

        let authorization = format!(
            "TC3-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.credential.secret_id, credential_scope, signed_headers, signature
        );

        request
            .headers_mut()
            .insert("Authorization", authorization.parse().unwrap());

        // println!("authorization = {}", authorization);

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TencentCloudResponseInnerError {
    #[serde(rename = "Code")]
    pub code: i64,
    #[serde(rename = "Message")]
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TencentCloudResponseInner {
    #[serde(rename = "RequestId")]
    pub request_id: String,
    #[serde(rename = "Error")]
    pub error: Option<TencentCloudResponseInnerError>,
    #[serde(flatten)]
    pub data: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TencentCloudResponse {
    #[serde(rename = "Response")]
    pub response: TencentCloudResponseInner,
}

impl TencentCloudResponse {
    pub fn is_success(&self) -> bool {
        self.response.error.is_none()
    }
}

impl RequestMethod {
    pub fn new_request(&self, client: &reqwest::Client, url: impl IntoUrl) -> RequestBuilder {
        match self {
            RequestMethod::GET => client.get(url),
            RequestMethod::POST => client.post(url),
            RequestMethod::PUT => client.put(url),
            RequestMethod::DELETE => client.delete(url),
            RequestMethod::PATCH => client.patch(url),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum HunyuanTranslationModel {
    #[serde(rename = "hunyuan-translation")]
    HunyuanTranslation,
    #[serde(rename = "hunyuan-translation-lite")]
    HunyuanTranslationLite,
}

impl TryFrom<String> for HunyuanTranslationModel {
    type Error = anyhow::Error;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        match value.as_str() {
            "hunyuan-translation" => Ok(HunyuanTranslationModel::HunyuanTranslation),
            "hunyuan-translation-lite" => Ok(HunyuanTranslationModel::HunyuanTranslationLite),
            _ => Err(anyhow!("Invalid model: {}", value)),
        }
    }
}

impl Display for HunyuanTranslationModel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            HunyuanTranslationModel::HunyuanTranslation => f.write_str("hunyuan-translation"),
            HunyuanTranslationModel::HunyuanTranslationLite => {
                f.write_str("hunyuan-translation-lite")
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum HunyuanTransLanguages {
    ///简体中文
    Zh,
    ///粤语
    Yue,
    ///英语
    En,
    ///法语
    Fr,
    ///葡萄牙语
    Pt,
    ///西班牙语
    Es,
    ///日语
    Ja,
    ///土耳其语
    Tr,
    ///俄语
    Ru,
    ///阿拉伯语
    Ar,
    ///韩语
    Ko,
    ///泰语
    Th,
    ///意大利语
    It,
    ///德语
    De,
    ///越南语
    Vi,
    ///马来语
    Ms,
    ///印尼语
    Id,
}

impl Display for HunyuanTransLanguages {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("{:?}", self).as_str())
    }
}

impl TryFrom<LanguageTag> for HunyuanTransLanguages {
    type Error = anyhow::Error;

    fn try_from(tag: LanguageTag) -> Result<Self, Self::Error> {
        let primary = tag.primary_language().to_ascii_lowercase();
        let primary = primary.as_str();
        match primary {
            "zh" => Ok(Self::Zh),
            "yue" => Ok(Self::Yue),
            "en" => Ok(Self::En),
            "fr" => Ok(Self::Fr),
            "pt" => Ok(Self::Pt),
            "es" => Ok(Self::Es),
            "ja" => Ok(Self::Ja),
            "tr" => Ok(Self::Tr),
            "ru" => Ok(Self::Ru),
            "ar" => Ok(Self::Ar),
            "ko" => Ok(Self::Ko),
            "th" => Ok(Self::Th),
            "it" => Ok(Self::It),
            "de" => Ok(Self::De),
            "vi" => Ok(Self::Vi),
            "ms" => Ok(Self::Ms),
            "id" => Ok(Self::Id),
            _ => bail!("Unsupported language tag: {}", tag),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HunyuanTranslator {
    pub model: HunyuanTranslationModel,
    pub secret_id: String,
    pub secret_key: String,
    pub region: Option<String>,
}

impl HunyuanTranslator {
    fn build_request(&self, task: &TranslateTask, stream: bool) -> Result<Value> {
        let source_language = task
            .source_language
            .clone()
            .ok_or(anyhow!(""))
            .and_then(|tag| tag.try_into())
            .map(|lang: HunyuanTransLanguages| lang.to_string().to_lowercase());

        let target_language = task
            .target_language
            .clone()
            .ok_or(anyhow!("缺少参数: target_language"))
            .and_then(|tag| tag.try_into())
            .map(|lang: HunyuanTransLanguages| lang.to_string().to_lowercase())?;

        let mut body = json!({
            "Model": self.model.to_string(),
            "Stream": stream,
            "Text": task.content.clone(),
            "Target": target_language,
        });

        if let Ok(source_lang) = source_language {
            body["Source"] = Value::String(source_lang);
        }

        if let Some(field) = &task.field {
            body["Field"] = Value::String(field.clone());
        }

        let mut references = vec![];

        if task.terms.len() > 0 {
            let mut list = task.terms[0..min(task.terms.len(), 10)]
                .to_vec()
                .iter()
                .map(|i| {
                    json!({
                        "Type": "term",
                        "Text": i.source.clone(),
                        "Translation": i.target.clone(),
                    })
                })
                .collect();
            references.append(&mut list);
        }

        if task.references.len() > 0 && references.len() < 10 {
            let mut list = task.references[0..min(task.references.len(), 10 - references.len())]
                .to_vec()
                .iter()
                .map(|i| {
                    json!({
                        "Type": "sentence",
                        "Text": i.source.clone(),
                        "Translation": i.target.clone(),
                    })
                })
                .collect();
            references.append(&mut list);
        }

        body["References"] = Value::Array(references);

        Ok(body)
    }
    fn lang_list() -> Result<Vec<String>> {
        Ok(vec![
            "zh".to_string(),
            "yue".to_string(),
            "en".to_string(),
            "fr".to_string(),
            "pt".to_string(),
            "es".to_string(),
            "ja".to_string(),
            "tr".to_string(),
            "ru".to_string(),
            "ar".to_string(),
            "ko".to_string(),
            "th".to_string(),
            "it".to_string(),
            "de".to_string(),
            "vi".to_string(),
            "ms".to_string(),
            "id".to_string(),
        ])
    }
}

#[async_trait]
impl Translator for HunyuanTranslator {
    type This = Self;

    async fn new(config: Value) -> Result<Self> {
        serde_json::from_value(config).map_err(|e| anyhow!(e))
    }

    fn get_supported_input_languages(&self) -> Result<Vec<String>> {
        HunyuanTranslator::lang_list()
    }

    fn get_supported_output_languages(&self) -> Result<Vec<String>> {
        HunyuanTranslator::lang_list()
    }

    fn is_supported_input_language(&self, lang: String) -> Result<bool> {
        Ok(HunyuanTransLanguages::try_from(LanguageTag::parse(lang.as_str())?).is_ok())
    }

    fn is_supported_output_language(&self, lang: String) -> Result<bool> {
        Ok(HunyuanTransLanguages::try_from(LanguageTag::parse(lang.as_str())?).is_ok())
    }

    async fn translate(&self, task: TranslateTask) -> Result<TranslateResult> {
        let client = Client::new();

        let tencent_request = TencentCloudRequest {
            host: "hunyuan.tencentcloudapi.com".to_string(),
            method: RequestMethod::POST,
            action: "ChatTranslations".to_string(),
            region: self.region.clone(),
            version: "2023-09-01".to_string(),
            language: None,
            credential: TencentCredential {
                secret_id: self.secret_id.clone(),
                secret_key: self.secret_key.clone(),
                token: None,
            },
            query: None,
            body: Some(self.build_request(&task, false)?),
        };

        let req = tencent_request.build_request(&client).unwrap();
        let resp = client.execute(req).await.map_err(|e| anyhow!(e))?;
        let json = resp.text().await.map_err(|e| anyhow!(e))?;

        let obj = serde_json::from_str::<TencentCloudResponse>(json.as_str())?;

        if !obj.is_success() {
            bail!("请求失败: {:?}", obj.response.error);
        }

        let data = obj.response.data.ok_or(anyhow!("数据解析失败"))?;

        let content = data["Choices"][0]["Message"]["Content"]
            .as_str()
            .map(|s| s.to_string());

        Ok(TranslateResult {
            reasoning: None,
            content,
        })
    }

    async fn translate_stream(
        &self,
        task: TranslateTask,
        sender: Sender<TranslateStreamChunk>,
    ) -> Result<()> {
        normal2stream(self, task, sender).await
    }
}

#[tokio::test]
async fn test_hunyuan() -> Result<()> {
    let translator = HunyuanTranslator {
        model: HunyuanTranslationModel::HunyuanTranslation,
        secret_id: env!("HUNYUAN_SECRET_ID").to_string(),
        secret_key: env!("HUNYUAN_SECRET_KEY").to_string(),
        region: None,
    };

    test_translate(translator).await
}

#[tokio::test]
async fn test_hunyuan_stream() -> Result<()> {
    let translator = HunyuanTranslator {
        model: HunyuanTranslationModel::HunyuanTranslation,
        secret_id: env!("HUNYUAN_SECRET_ID").to_string(),
        secret_key: env!("HUNYUAN_SECRET_KEY").to_string(),
        region: None,
    };

    test_translate_stream(translator).await
}
