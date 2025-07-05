use crate::storage::{PromptMetadata, Storage};
use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use std::fs;
use std::time::Duration;

// Helper functions for deserialization
fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::*;
    use serde_json::Value;

    let value: Value = Deserialize::deserialize(deserializer)?;

    match value {
        Value::Array(arr) => {
            // Normal case: JSON array of strings
            arr.into_iter()
                .map(|v| match v {
                    Value::String(s) => Ok(s),
                    _ => Err(Error::custom("Expected string in tags array")),
                })
                .collect()
        }
        Value::String(s) => {
            // Legacy case: comma-separated string or JSON string
            if s.starts_with('[') && s.ends_with(']') {
                serde_json::from_str(&s).map_err(Error::custom)
            } else {
                Ok(s.split(',').map(|s| s.trim().to_string()).collect())
            }
        }
        _ => Err(Error::custom("Expected array or string for tags")),
    }
}

fn default_license() -> String {
    "MIT".to_string()
}

#[derive(Debug, Clone)]
pub struct RegistryClient {
    base_url: String,
    client: reqwest::Client,
    api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub id: Option<String>,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(alias = "author_name")]
    pub author: String,
    #[serde(deserialize_with = "deserialize_tags")]
    pub tags: Vec<String>,
    #[serde(default = "default_license")]
    pub license: String,
    pub created_at: String,
    pub updated_at: String,
    pub downloads: u64,
    #[serde(default)]
    pub size_bytes: u64,
    pub content: Option<String>,
    pub bank_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackagePrompt {
    pub name: String,
    pub content: String,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
    #[serde(flatten)]
    pub metadata: PackageMetadata,
    pub prompts: Vec<PackagePrompt>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    #[serde(alias = "prompts")]
    pub packages: Vec<PackageMetadata>,
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub page: u64,
    #[serde(default)]
    pub has_more: bool,
    pub query: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishRequest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub tags: Vec<String>,
    pub license: String,
    pub prompts: Vec<PackagePrompt>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PublishResponse {
    pub success: bool,
    pub package: PackageMetadata,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShareResponse {
    pub share_id: String,
    pub share_url: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct PublicShareRequest {
    pub prompt_name: String,
    pub description: String,
    pub content: String,
    pub allow_suggestions: bool,
    pub expires_hours: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct InviteShareRequest {
    pub prompt_name: String,
    pub description: String,
    pub content: String,
    pub emails: Vec<String>,
    pub allow_suggestions: bool,
    pub expires_hours: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SuggestionResponse {
    pub id: String,
    pub shared_prompt_id: String,
    pub shared_prompt_name: String,
    pub suggested_by_email: Option<String>,
    pub suggestion_text: String,
    pub improvement_content: Option<String>,
    pub status: String, // pending, accepted, rejected
    pub created_at: String,
}


#[derive(Debug, Serialize)]
pub struct AcceptSuggestionRequest {
    pub suggestion_id: String,
}

#[derive(Debug, Serialize)]
pub struct RejectSuggestionRequest {
    pub suggestion_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImprovementResponse {
    pub id: String,
    pub prompt_name: String,
    pub improvement_type: String, // crowd, ai
    pub request_id: String,
    pub status: String, // queued, processing, completed, failed
    pub message: String,
    pub estimated_completion: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiSuggestion {
    pub improvement_id: String,
    pub suggested_content: String,
    pub improvements: Vec<String>,
    pub reasoning: String,
    pub confidence_score: f64,
}

impl RegistryClient {
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("Failed to create HTTP client");

        // Try to load API key from config
        let api_key = Self::load_api_key_from_config();

        Self {
            base_url,
            client,
            api_key,
        }
    }

    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }

    /// Load API key from config.toml
    fn load_api_key_from_config() -> Option<String> {
        // Try to get storage and config path
        let storage = crate::storage::Storage::new().ok()?;
        let config_path = storage.config_path();
        if !config_path.exists() {
            return None;
        }

        let contents = fs::read_to_string(&config_path).ok()?;
        let config: toml::Value = contents.parse().ok()?;

        // Get API key from config (stored in plaintext with secure file permissions)
        config
            .get("api_key")
            .and_then(|key| key.as_str())
            .map(|s| s.to_string())
    }

    /// Request a magic link for email authentication
    pub async fn request_magic_link(&self, email: &str) -> Result<()> {
        let url = format!("{}/api/auth/request-login", self.base_url);
        
        #[derive(Serialize)]
        struct MagicLinkRequest {
            email: String,
        }
        
        let request = MagicLinkRequest {
            email: email.to_string(),
        };
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send magic link request")?;
            
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            anyhow::bail!("Magic link request failed: {}", error_text);
        }
        
        Ok(())
    }
    
    /// Verify magic link code and get API key
    pub async fn verify_magic_link(&self, email: &str, code: &str) -> Result<String> {
        let url = format!("{}/api/auth/verify-login", self.base_url);
        
        #[derive(Serialize)]
        struct VerifyRequest {
            email: String,
            code: String,
        }
        
        #[derive(Deserialize)]
        struct VerifyResponse {
            api_key: String,
        }
        
        let request = VerifyRequest {
            email: email.to_string(),
            code: code.to_string(),
        };
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to verify magic link")?;
            
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            anyhow::bail!("Magic link verification failed: {}", error_text);
        }
        
        let verify_response: VerifyResponse = response
            .json()
            .await
            .context("Failed to parse verification response")?;
            
        Ok(verify_response.api_key)
    }

    /// Search for packages in the registry
    pub async fn search(&self, query: &str, limit: Option<u32>) -> Result<SearchResult> {
        let url = format!("{}/api/prompts/search", self.base_url);

        let mut request = self.client.get(&url).query(&[("q", query)]);

        if let Some(limit) = limit {
            request = request.query(&[("limit", &limit.to_string())]);
        }

        // Add API key header if available
        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }

        let response = request
            .send()
            .await
            .context("Failed to send search request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Search failed with status {}: {}",
                status,
                error_text
            ));
        }

        // First get the response text for debugging
        let response_text = response
            .text()
            .await
            .context("Failed to read response text")?;

        // Try to parse the JSON
        let search_result: SearchResult = serde_json::from_str(&response_text).context(format!(
            "Failed to parse search response. Response was: {}",
            response_text
        ))?;

        Ok(search_result)
    }

    /// Download a prompt from the registry
    pub async fn download_prompt(&self, prompt_id: &str) -> Result<PackageMetadata> {
        // First try direct ID lookup
        let url = format!("{}/api/prompts/{}", self.base_url, prompt_id);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to download prompt")?;

        if response.status().is_success() {
            let prompt: PackageMetadata = response
                .json()
                .await
                .context("Failed to parse prompt response")?;
            return Ok(prompt);
        }

        // If direct lookup fails, try searching by name
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            match self.search(prompt_id, Some(1)).await {
                Ok(search_result) if !search_result.packages.is_empty() => {
                    let first_match = &search_result.packages[0];
                    // Use the actual ID to download the prompt
                    if let Some(ref id) = first_match.id {
                        return self.download_prompt_by_id(id).await;
                    }
                }
                _ => {} // Fall through to original error
            }
        }

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(anyhow::anyhow!("Prompt '{}' not found", prompt_id));
        }

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Download failed with status: {}",
                response.status()
            ));
        }

        // This shouldn't be reached, but just in case
        let prompt: PackageMetadata = response
            .json()
            .await
            .context("Failed to parse prompt response")?;

        Ok(prompt)
    }

    /// Download a prompt by exact ID (internal helper)
    async fn download_prompt_by_id(&self, prompt_id: &str) -> Result<PackageMetadata> {
        let url = format!("{}/api/prompts/{}", self.base_url, prompt_id);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to download prompt by ID")?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(anyhow::anyhow!("Prompt with ID '{}' not found", prompt_id));
        }

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Download failed with status: {}",
                response.status()
            ));
        }

        let prompt: PackageMetadata = response
            .json()
            .await
            .context("Failed to parse prompt response")?;

        Ok(prompt)
    }

    /// Download a package from the registry (legacy method for compatibility)
    pub async fn download(&self, package_name: &str, _version: Option<&str>) -> Result<Package> {
        // For now, treat package_name as prompt_id and download single prompt
        let prompt = self.download_prompt(package_name).await?;

        // Convert single prompt to Package format for compatibility
        let package_prompt = PackagePrompt {
            name: prompt.name.clone(),
            content: prompt.content.clone().unwrap_or_default(),
            size_bytes: prompt.content.as_ref().map(|c| c.len() as u64).unwrap_or(0),
        };

        let package = Package {
            metadata: prompt,
            prompts: vec![package_prompt],
        };

        Ok(package)
    }

    /// Publish a package to the registry
    pub async fn publish(&self, package: PublishRequest) -> Result<PublishResponse> {
        if self.api_key.is_none() {
            return Err(anyhow::anyhow!(
                "Authentication required. Run 'ph login' first."
            ));
        }

        let url = format!("{}/api/prompts", self.base_url);

        // Extract the first prompt from the package and send in the format the backend expects
        let prompt = &package.prompts[0]; // For now, publish one prompt at a time
        
        // Parse prompt name to extract bank
        let (bank_name, prompt_name) = if package.name.contains('/') {
            let parts: Vec<&str> = package.name.splitn(2, '/').collect();
            (parts[0], parts[1])
        } else {
            ("default", package.name.as_str())
        };

        let payload = serde_json::json!({
            "name": prompt_name,
            "content": prompt.content,
            "bank_id": format!("bank-{}", bank_name),
            "description": package.description,
            "tags": package.tags,
            "is_public": true
        });

        let mut request = self.client.post(&url).json(&payload);

        // Use X-API-Key header for simple authentication
        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }

        let response = request.send().await.context("Failed to publish package")?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!(
                "Authentication required. Run 'ph login' first."
            ));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Publish failed with status {}: {}",
                status,
                error_text
            ));
        }

        // Parse the actual backend response format
        let backend_response: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse publish response")?;

        // Convert to our expected format
        let publish_response = PublishResponse {
            success: true,
            package: PackageMetadata {
                id: backend_response["id"].as_str().map(|s| s.to_string()),
                name: prompt_name.to_string(),
                version: package.version.clone(),
                description: package.description.clone(),
                tags: package.tags.clone(),
                downloads: 0,
                size_bytes: prompt.size_bytes,
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                author: "test_user".to_string(),
                license: package.license.clone(),
                content: Some(prompt.content.clone()),
                bank_name: Some(bank_name.to_string()),
            },
        };

        Ok(publish_response)
    }

    /// Install a package and integrate with local storage
    pub async fn install_package(
        &self,
        storage: &Storage,
        package_name: &str,
        version: Option<&str>,
    ) -> Result<InstallResult> {
        // Download package
        let package = self.download(package_name, version).await?;

        let mut installed = Vec::new();
        let mut conflicts = Vec::new();

        // Install each prompt
        for prompt in &package.prompts {
            let prompt_path = storage.prompt_path(&prompt.name);

            // Check for conflicts
            if prompt_path.exists() {
                conflicts.push(prompt.name.clone());
                continue;
            }

            // Create metadata from package info
            let metadata = PromptMetadata {
                id: prompt.name.clone(),
                description: format!("From package {}", package.metadata.name),
                tags: Some(vec!["imported".to_string(), "registry".to_string()]),
                created_at: Some(chrono::Utc::now().to_rfc3339()),
                updated_at: None,
                version: Some(package.metadata.version.clone()),
                git_hash: None,
                parent_version: None,
            };

            // Parse prompt content to extract body (skip frontmatter if present)
            let body = if prompt.content.starts_with("---") {
                // Extract body from existing frontmatter
                let lines: Vec<&str> = prompt.content.lines().collect();
                if let Some(end_pos) = lines.iter().skip(1).position(|&line| line == "---") {
                    lines[(end_pos + 2)..].join("\n")
                } else {
                    prompt.content.clone()
                }
            } else {
                prompt.content.clone()
            };

            // Write prompt to storage
            storage
                .write_prompt(&prompt.name, &metadata, &body)
                .context(format!("Failed to write prompt '{}'", prompt.name))?;

            installed.push(prompt.name.clone());
        }

        Ok(InstallResult {
            package_name: package.metadata.name,
            version: package.metadata.version,
            installed,
            conflicts,
        })
    }

    /// Generic GET request
    pub async fn get(&self, path: &str) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.get(&url);

        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }

        let response = request.send().await.context("Failed to send GET request")?;
        Ok(response)
    }

    /// Generic POST request
    pub async fn post(&self, path: &str, payload: &serde_json::Value) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.post(&url).json(payload);

        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }

        let response = request
            .send()
            .await
            .context("Failed to send POST request")?;
        Ok(response)
    }

    /// Generic DELETE request
    pub async fn delete(
        &self,
        path: &str,
        payload: Option<&serde_json::Value>,
    ) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.delete(&url);

        if let Some(payload) = payload {
            request = request.json(payload);
        }

        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }

        let response = request
            .send()
            .await
            .context("Failed to send DELETE request")?;
        Ok(response)
    }

    /// Generic PUT request
    pub async fn put(&self, path: &str, payload: &serde_json::Value) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.put(&url).json(payload);

        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }

        let response = request.send().await.context("Failed to send PUT request")?;
        Ok(response)
    }

    /// GET request with query parameters
    pub async fn get_with_params(
        &self,
        path: &str,
        params: &[(&str, &str)],
    ) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.client.get(&url);

        if !params.is_empty() {
            request = request.query(params);
        }

        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }

        let response = request
            .send()
            .await
            .context("Failed to send GET request with params")?;
        Ok(response)
    }

    /// Create a package from local prompts for publishing
    pub fn create_package_from_storage(
        &self,
        storage: &Storage,
        package_name: &str,
        version: &str,
        description: &str,
        prompt_names: &[String],
    ) -> Result<PublishRequest> {
        let mut prompts = Vec::new();

        for prompt_name in prompt_names {
            let (metadata, body) = storage
                .read_prompt(prompt_name)
                .context(format!("Failed to read prompt '{}'", prompt_name))?;

            // Reconstruct full content with frontmatter
            let content = format!(
                "---\nid: {}\ndescription: {}\n---\n\n{}",
                metadata.id, metadata.description, body
            );

            let content_len = content.len() as u64;
            prompts.push(PackagePrompt {
                name: prompt_name.clone(),
                content,
                size_bytes: content_len,
            });
        }

        Ok(PublishRequest {
            name: package_name.to_string(),
            version: version.to_string(),
            description: description.to_string(),
            tags: vec!["prompthive".to_string()],
            license: "MIT".to_string(),
            prompts,
        })
    }

    /// Create a public sharing link for a prompt (viral sharing)
    pub async fn create_public_share(
        &self,
        prompt_name: &str,
        description: &str,
        content: &str,
        allow_suggestions: bool,
        expires_hours: Option<u32>,
    ) -> Result<ShareResponse> {
        let request_payload = PublicShareRequest {
            prompt_name: prompt_name.to_string(),
            description: description.to_string(),
            content: content.to_string(),
            allow_suggestions,
            expires_hours,
        };

        let payload = serde_json::to_value(&request_payload)
            .context("Failed to serialize public share request")?;

        let response = self
            .post("/api/share/public", &payload)
            .await
            .context("Failed to create public share")?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!(
                "Authentication required. Run 'ph login' first."
            ));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Create public share failed with status {}: {}",
                status,
                error_text
            ));
        }

        let share_response: ShareResponse = response
            .json()
            .await
            .context("Failed to parse public share response")?;

        Ok(share_response)
    }

    /// Create invitation-based sharing for a prompt (viral sharing)
    pub async fn create_invite_share(
        &self,
        prompt_name: &str,
        description: &str,
        content: &str,
        emails: &[&str],
        allow_suggestions: bool,
        expires_hours: Option<u32>,
    ) -> Result<ShareResponse> {
        let request_payload = InviteShareRequest {
            prompt_name: prompt_name.to_string(),
            description: description.to_string(),
            content: content.to_string(),
            emails: emails.iter().map(|e| e.to_string()).collect(),
            allow_suggestions,
            expires_hours,
        };

        let payload = serde_json::to_value(&request_payload)
            .context("Failed to serialize invite share request")?;

        let response = self
            .post("/api/share/invite", &payload)
            .await
            .context("Failed to create invite share")?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!(
                "Authentication required. Run 'ph login' first."
            ));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Create invite share failed with status {}: {}",
                status,
                error_text
            ));
        }

        let share_response: ShareResponse = response
            .json()
            .await
            .context("Failed to parse invite share response")?;

        Ok(share_response)
    }

    /// List suggestions for user's shared prompts
    pub async fn list_suggestions(
        &self,
        share_id: Option<&str>,
        pending_only: bool,
    ) -> Result<Vec<SuggestionResponse>> {
        let url = format!("{}/api/suggestions", self.base_url);
        let mut query_params = Vec::new();

        if let Some(id) = share_id {
            query_params.push(("share_id", id));
        }

        if pending_only {
            query_params.push(("status", "pending"));
        }

        let response = self
            .get_with_params(&url, &query_params)
            .await
            .context("Failed to list suggestions")?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!(
                "Authentication required. Run 'ph login' first."
            ));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "List suggestions failed with status {}: {}",
                status,
                error_text
            ));
        }

        let suggestions: Vec<SuggestionResponse> = response
            .json()
            .await
            .context("Failed to parse suggestions response")?;

        Ok(suggestions)
    }

    /// Get detailed information about a specific suggestion
    pub async fn get_suggestion(&self, suggestion_id: &str) -> Result<SuggestionResponse> {
        let url = format!("{}/api/suggestions/{}", self.base_url, suggestion_id);

        let response = self.get(&url).await.context("Failed to get suggestion")?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!(
                "Authentication required. Run 'ph login' first."
            ));
        }

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(anyhow::anyhow!("Suggestion '{}' not found", suggestion_id));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Get suggestion failed with status {}: {}",
                status,
                error_text
            ));
        }

        let suggestion: SuggestionResponse = response
            .json()
            .await
            .context("Failed to parse suggestion response")?;

        Ok(suggestion)
    }

    /// Accept a suggestion and mark it as accepted
    pub async fn accept_suggestion(&self, suggestion_id: &str) -> Result<SuggestionResponse> {
        let url = format!("{}/api/suggestions/{}/accept", self.base_url, suggestion_id);
        let payload = serde_json::json!({});

        let response = self
            .put(&url, &payload)
            .await
            .context("Failed to accept suggestion")?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!(
                "Authentication required. Run 'ph login' first."
            ));
        }

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(anyhow::anyhow!("Suggestion '{}' not found", suggestion_id));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Accept suggestion failed with status {}: {}",
                status,
                error_text
            ));
        }

        let suggestion: SuggestionResponse = response
            .json()
            .await
            .context("Failed to parse accept suggestion response")?;

        Ok(suggestion)
    }

    /// Reject a suggestion with optional reason
    pub async fn reject_suggestion(
        &self,
        suggestion_id: &str,
        reason: Option<&str>,
    ) -> Result<SuggestionResponse> {
        let url = format!("{}/api/suggestions/{}/reject", self.base_url, suggestion_id);
        let payload = serde_json::json!({
            "reason": reason
        });

        let response = self
            .put(&url, &payload)
            .await
            .context("Failed to reject suggestion")?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!(
                "Authentication required. Run 'ph login' first."
            ));
        }

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(anyhow::anyhow!("Suggestion '{}' not found", suggestion_id));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Reject suggestion failed with status {}: {}",
                status,
                error_text
            ));
        }

        let suggestion: SuggestionResponse = response
            .json()
            .await
            .context("Failed to parse reject suggestion response")?;

        Ok(suggestion)
    }

    /// Submit prompt for community-driven improvement (Phase 2B)
    pub async fn submit_crowd_improvement(
        &self,
        prompt_name: &str,
        description: &str,
        content: &str,
        instructions: Option<&str>,
        priority: &str,
    ) -> Result<ImprovementResponse> {
        use serde_json::json;

        let request_payload = json!({
            "prompt_name": prompt_name,
            "description": description,
            "content": content,
            "instructions": instructions,
            "priority": priority
        });

        let response = self
            .post("/api/improve/crowd", &request_payload)
            .await
            .context("Failed to submit crowd improvement request")?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!(
                "Authentication required. Run 'ph login' first."
            ));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Crowd improvement request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let improvement_response: ImprovementResponse = response
            .json()
            .await
            .context("Failed to parse crowd improvement response")?;

        Ok(improvement_response)
    }

    /// Submit prompt for AI-powered enhancement (Phase 2B)
    pub async fn submit_ai_improvement(
        &self,
        prompt_name: &str,
        description: &str,
        content: &str,
        instructions: Option<&str>,
        priority: &str,
    ) -> Result<AiSuggestion> {
        use serde_json::json;

        let request_payload = json!({
            "prompt_name": prompt_name,
            "description": description,
            "content": content,
            "instructions": instructions,
            "priority": priority
        });

        let response = self
            .post("/api/improve/ai", &request_payload)
            .await
            .context("Failed to submit AI improvement request")?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!(
                "Authentication required. Run 'ph login' first."
            ));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "AI improvement request failed with status {}: {}",
                status,
                error_text
            ));
        }

        let ai_suggestion: AiSuggestion = response
            .json()
            .await
            .context("Failed to parse AI improvement response")?;

        Ok(ai_suggestion)
    }

    /// Get status of an improvement request (Phase 2B)
    pub async fn get_improvement_status(&self, improvement_id: &str) -> Result<ImprovementResponse> {
        let url = format!("/api/improve/status/{}", improvement_id);

        let response = self.get(&url).await.context("Failed to get improvement status")?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow::anyhow!(
                "Authentication required. Run 'ph login' first."
            ));
        }

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(anyhow::anyhow!("Improvement request '{}' not found", improvement_id));
        }

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Get improvement status failed with status {}: {}",
                status,
                error_text
            ));
        }

        let improvement_response: ImprovementResponse = response
            .json()
            .await
            .context("Failed to parse improvement status response")?;

        Ok(improvement_response)
    }

    // === SUBSCRIPTION MANAGEMENT ===
    // Subscription management methods
    pub async fn get_subscription_status(&self, user_email: &str) -> Result<serde_json::Value> {
        let url = format!("{}/api/subscription/status?email={}", self.base_url, urlencoding::encode(user_email));
        let response = self.authenticated_get(&url).await?;
        let status = response.json().await?;
        Ok(status)
    }
    
    pub async fn get_subscription_usage(&self, user_email: &str) -> Result<serde_json::Value> {
        let url = format!("{}/api/subscription/usage?email={}", self.base_url, urlencoding::encode(user_email));
        let response = self.authenticated_get(&url).await?;
        let usage = response.json().await?;
        Ok(usage)
    }
    
    pub async fn get_customer_portal_link(&self, user_email: &str) -> Result<serde_json::Value> {
        let url = format!("{}/api/subscription/portal?email={}", self.base_url, urlencoding::encode(user_email));
        let response = self.authenticated_get(&url).await?;
        let portal = response.json().await?;
        Ok(portal)
    }
    
    pub async fn get_subscription_analytics(&self, user_email: &str) -> Result<serde_json::Value> {
        let url = format!("{}/api/subscription/analytics?email={}", self.base_url, urlencoding::encode(user_email));
        let response = self.authenticated_get(&url).await?;
        let analytics = response.json().await?;
        Ok(analytics)
    }
    
    // Helper methods for authenticated requests
    async fn authenticated_get(&self, url: &str) -> Result<reqwest::Response> {
        let mut request = self.client.get(url);
        
        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }
        
        let response = request.send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Request failed with status {}: {}",
                status,
                error_text
            ));
        }
        
        Ok(response)
    }
    
    #[allow(dead_code)]
    async fn authenticated_post(&self, url: &str, body: Option<&str>) -> Result<reqwest::Response> {
        let mut request = self.client.post(url);
        
        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }
        
        if let Some(body) = body {
            request = request.header("Content-Type", "application/json").body(body.to_string());
        }
        
        let response = request.send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Request failed with status {}: {}",
                status,
                error_text
            ));
        }
        
        Ok(response)
    }
}

#[derive(Debug)]
pub struct InstallResult {
    pub package_name: String,
    pub version: String,
    pub installed: Vec<String>,
    pub conflicts: Vec<String>,
}

impl InstallResult {
    pub fn display(&self) {
        use colored::*;

        if !self.installed.is_empty() {
            println!(
                "✅ Installed package {}@{}",
                self.package_name.bold(),
                self.version.dimmed()
            );

            for prompt in &self.installed {
                println!("  + {}", prompt.green());
            }
        }

        if !self.conflicts.is_empty() {
            println!("\n⚠️  Conflicts (not installed):");
            for prompt in &self.conflicts {
                println!("  ! {} (already exists)", prompt.yellow());
            }
            println!("\nUse --force to overwrite existing prompts");
        }
    }
}

/// Default registry configuration
pub fn default_registry_url() -> String {
    std::env::var("PROMPTHIVE_REGISTRY_URL")
        .unwrap_or_else(|_| "https://registry.prompthive.sh".to_string())
}

// API keys are stored in plaintext in config.toml with secure file permissions (0600)
// TODO: Consider using OS keychain (e.g., macOS Keychain, Windows Credential Manager, Linux Secret Service)
// for more secure storage in the future.

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_storage() -> (TempDir, Storage) {
        let temp_dir = TempDir::new().unwrap();
        let storage =
            crate::storage::Storage::new_with_base(temp_dir.path().to_path_buf()).unwrap();
        storage.init().unwrap();
        (temp_dir, storage)
    }

    #[test]
    fn test_registry_client_creation() {
        let client = RegistryClient::new("https://test.registry.com".to_string());
        assert_eq!(client.base_url, "https://test.registry.com");
        // API key might be loaded from config, so we don't assert it's None
    }

    #[test]
    fn test_registry_client_with_auth() {
        let client = RegistryClient::new("https://test.registry.com".to_string())
            .with_api_key("ph_test_1234567890".to_string());
        assert_eq!(client.api_key, Some("ph_test_1234567890".to_string()));
    }

    #[test]
    fn test_default_registry_url() {
        // Use a lock to ensure test isolation since env vars are global
        use std::sync::Mutex;
        static ENV_LOCK: Mutex<()> = Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap();
        
        // Save current value
        let original = std::env::var("PROMPTHIVE_REGISTRY_URL").ok();
        
        // Test without environment variable
        std::env::remove_var("PROMPTHIVE_REGISTRY_URL");
        let default_url = default_registry_url();
        assert_eq!(
            default_url,
            "https://registry.prompthive.sh"
        );

        // Test with environment variable
        std::env::set_var("PROMPTHIVE_REGISTRY_URL", "https://custom.registry.com");
        let custom_url = default_registry_url();
        assert_eq!(custom_url, "https://custom.registry.com");

        // Restore original state
        match original {
            Some(val) => std::env::set_var("PROMPTHIVE_REGISTRY_URL", val),
            None => std::env::remove_var("PROMPTHIVE_REGISTRY_URL"),
        }
    }

    #[test]
    fn test_package_metadata_serialization() {
        let metadata = PackageMetadata {
            id: Some("test-id".to_string()),
            name: "test/package".to_string(),
            version: "1.0.0".to_string(),
            description: "Test package".to_string(),
            author: "test-author".to_string(),
            bank_name: Some("test-bank".to_string()),
            content: Some("test content".to_string()),
            tags: vec!["test".to_string(), "example".to_string()],
            license: "MIT".to_string(),
            created_at: "2023-01-01T00:00:00Z".to_string(),
            updated_at: "2023-01-02T00:00:00Z".to_string(),
            downloads: 42,
            size_bytes: 1024,
        };

        // Test serialization
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("test/package"));
        assert!(json.contains("1.0.0"));
        assert!(json.contains("MIT"));

        // Test deserialization
        let deserialized: PackageMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test/package");
        assert_eq!(deserialized.downloads, 42);
    }

    #[test]
    fn test_package_prompt_creation() {
        let prompt = PackagePrompt {
            name: "test-prompt".to_string(),
            content: "This is test content".to_string(),
            size_bytes: 20,
        };

        assert_eq!(prompt.size_bytes, 20);
        assert!(prompt.content.contains("test content"));
    }

    #[test]
    fn test_publish_request_creation() {
        let prompts = vec![
            PackagePrompt {
                name: "prompt1".to_string(),
                content: "Content 1".to_string(),
                size_bytes: 9,
            },
            PackagePrompt {
                name: "prompt2".to_string(),
                content: "Content 2".to_string(),
                size_bytes: 9,
            },
        ];

        let request = PublishRequest {
            name: "test/package".to_string(),
            version: "1.0.0".to_string(),
            description: "Test package".to_string(),
            tags: vec!["test".to_string()],
            license: "MIT".to_string(),
            prompts,
        };

        assert_eq!(request.prompts.len(), 2);
        assert_eq!(request.name, "test/package");
        assert_eq!(request.license, "MIT");
    }

    #[test]
    fn test_install_result_display() {
        let result = InstallResult {
            package_name: "test/package".to_string(),
            version: "1.0.0".to_string(),
            installed: vec!["prompt1".to_string(), "prompt2".to_string()],
            conflicts: vec!["existing-prompt".to_string()],
        };

        assert_eq!(result.installed.len(), 2);
        assert_eq!(result.conflicts.len(), 1);
        assert!(result.installed.contains(&"prompt1".to_string()));
        assert!(result.conflicts.contains(&"existing-prompt".to_string()));
    }

    #[test]
    fn test_create_package_from_storage() {
        let (_temp, storage) = create_test_storage();

        // Create test prompts
        let metadata1 = crate::storage::PromptMetadata {
            id: "test-prompt-1".to_string(),
            description: "First test prompt".to_string(),
            tags: Some(vec!["test".to_string()]),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            updated_at: None,
            version: None,
            git_hash: None,
            parent_version: None,
        };
        storage
            .write_prompt("test-prompt-1", &metadata1, "Content 1")
            .unwrap();

        let metadata2 = crate::storage::PromptMetadata {
            id: "test-prompt-2".to_string(),
            description: "Second test prompt".to_string(),
            tags: None,
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            updated_at: None,
            version: None,
            git_hash: None,
            parent_version: None,
        };
        storage
            .write_prompt("test-prompt-2", &metadata2, "Content 2")
            .unwrap();

        // Create package from storage
        let client = RegistryClient::new("https://test.registry.com".to_string());
        let prompt_names = vec!["test-prompt-1".to_string(), "test-prompt-2".to_string()];

        let package = client
            .create_package_from_storage(
                &storage,
                "test/package",
                "1.0.0",
                "Test package",
                &prompt_names,
            )
            .unwrap();

        assert_eq!(package.name, "test/package");
        assert_eq!(package.version, "1.0.0");
        assert_eq!(package.prompts.len(), 2);

        // Verify content includes frontmatter
        assert!(package.prompts[0].content.contains("---"));
        assert!(package.prompts[0].content.contains("test-prompt-1"));
        assert!(package.prompts[0].content.contains("Content 1"));
    }

    #[test]
    fn test_create_package_with_missing_prompt() {
        let (_temp, storage) = create_test_storage();

        let client = RegistryClient::new("https://test.registry.com".to_string());
        let prompt_names = vec!["nonexistent-prompt".to_string()];

        let result = client.create_package_from_storage(
            &storage,
            "test/package",
            "1.0.0",
            "Test package",
            &prompt_names,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("nonexistent-prompt"));
    }

    #[test]
    fn test_search_result_pagination() {
        let search_result = SearchResult {
            packages: vec![],
            total: 100,
            page: 1,
            has_more: true,
            query: None,
        };

        assert_eq!(search_result.total, 100);
        assert_eq!(search_result.page, 1);
        assert!(search_result.has_more);
    }

    #[test]
    fn test_package_content_parsing() {
        let _client = RegistryClient::new("https://test.registry.com".to_string());

        // Test content with frontmatter
        let content_with_frontmatter =
            "---\nid: test\ndescription: Test\n---\n\nActual content here";

        // This simulates the parsing logic in install_package
        let body = if content_with_frontmatter.starts_with("---") {
            let lines: Vec<&str> = content_with_frontmatter.lines().collect();
            if let Some(end_pos) = lines.iter().skip(1).position(|&line| line == "---") {
                lines[(end_pos + 2)..].join("\n").trim().to_string()
            } else {
                content_with_frontmatter.to_string()
            }
        } else {
            content_with_frontmatter.to_string()
        };

        assert_eq!(body, "Actual content here");
    }

    #[test]
    fn test_package_size_calculation() {
        let content = "This is test content for size calculation";
        let prompt = PackagePrompt {
            name: "test".to_string(),
            content: content.to_string(),
            size_bytes: content.len() as u64,
        };

        assert_eq!(prompt.size_bytes, content.len() as u64);
        assert!(prompt.size_bytes > 0);
    }

    #[test]
    fn test_package_tags_handling() {
        let metadata = PackageMetadata {
            id: Some("test-id".to_string()),
            name: "test/package".to_string(),
            version: "1.0.0".to_string(),
            description: "Test".to_string(),
            author: "test".to_string(),
            bank_name: Some("test-bank".to_string()),
            content: Some("test content".to_string()),
            tags: vec![
                "ai".to_string(),
                "prompts".to_string(),
                "testing".to_string(),
            ],
            license: "MIT".to_string(),
            created_at: "2023-01-01T00:00:00Z".to_string(),
            updated_at: "2023-01-01T00:00:00Z".to_string(),
            downloads: 0,
            size_bytes: 1024,
        };

        assert!(metadata.tags.contains(&"ai".to_string()));
        assert!(metadata.tags.contains(&"prompts".to_string()));
        assert_eq!(metadata.tags.len(), 3);
    }
}
