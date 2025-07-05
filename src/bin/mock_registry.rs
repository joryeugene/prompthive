use actix_web::{web, App, HttpResponse, HttpServer, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize, Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PackageMetadata {
    name: String,
    version: String,
    description: String,
    author: String,
    tags: Vec<String>,
    license: String,
    created_at: String,
    updated_at: String,
    downloads: u64,
    size_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PackagePrompt {
    name: String,
    content: String,
    size_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Package {
    #[serde(flatten)]
    metadata: PackageMetadata,
    prompts: Vec<PackagePrompt>,
}

async fn search_packages(query: web::Query<SearchQuery>) -> Result<HttpResponse> {
    let mock_packages = vec![
        PackageMetadata {
            name: "test/essentials".to_string(),
            version: "1.0.0".to_string(),
            description: "Essential productivity prompts".to_string(),
            author: "test-user".to_string(),
            tags: vec!["productivity".to_string(), "essentials".to_string()],
            license: "MIT".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
            downloads: 42,
            size_bytes: 2048,
        },
        PackageMetadata {
            name: "test/ai-tools".to_string(),
            version: "2.1.0".to_string(),
            description: "AI-powered development prompts".to_string(),
            author: "test-user".to_string(),
            tags: vec!["ai".to_string(), "development".to_string()],
            license: "MIT".to_string(),
            created_at: "2025-01-02T00:00:00Z".to_string(),
            updated_at: "2025-01-03T00:00:00Z".to_string(),
            downloads: 128,
            size_bytes: 4096,
        },
        PackageMetadata {
            name: "team/shared".to_string(),
            version: "0.5.0".to_string(),
            description: "Shared team prompts for collaboration".to_string(),
            author: "team-lead".to_string(),
            tags: vec!["team".to_string(), "collaboration".to_string()],
            license: "Apache-2.0".to_string(),
            created_at: "2025-01-04T00:00:00Z".to_string(),
            updated_at: "2025-01-05T00:00:00Z".to_string(),
            downloads: 15,
            size_bytes: 1024,
        },
    ];

    // Simple search filter
    let filtered: Vec<_> = mock_packages
        .into_iter()
        .filter(|p| {
            let query_lower = query.q.to_lowercase();
            p.name.to_lowercase().contains(&query_lower)
                || p.description.to_lowercase().contains(&query_lower)
                || p.tags
                    .iter()
                    .any(|t| t.to_lowercase().contains(&query_lower))
        })
        .take(query.limit.unwrap_or(10) as usize)
        .collect();

    let total = filtered.len() as u64;
    let has_more = false;

    Ok(HttpResponse::Ok().json(json!({
        "packages": filtered,
        "total": total,
        "page": 1,
        "has_more": has_more
    })))
}

async fn get_package(path: web::Path<String>) -> Result<HttpResponse> {
    let package_name = path.into_inner();

    // Mock package data
    let packages = vec![
        (
            "test/essentials",
            Package {
                metadata: PackageMetadata {
                    name: "test/essentials".to_string(),
                    version: "1.0.0".to_string(),
                    description: "Essential productivity prompts".to_string(),
                    author: "test-user".to_string(),
                    tags: vec!["productivity".to_string(), "essentials".to_string()],
                    license: "MIT".to_string(),
                    created_at: "2025-01-01T00:00:00Z".to_string(),
                    updated_at: "2025-01-01T00:00:00Z".to_string(),
                    downloads: 42,
                    size_bytes: 2048,
                },
                prompts: vec![
                    PackagePrompt {
                        name: "commit-message".to_string(),
                        content: "---\nid: commit-message\ndescription: Generate perfect git commit messages\n---\n\nGenerate a concise, semantic commit message for the following changes:\n\n{input}\n\nFollow conventional commits format (feat/fix/docs/style/refactor/test/chore).".to_string(),
                        size_bytes: 256,
                    },
                    PackagePrompt {
                        name: "code-review".to_string(),
                        content: "---\nid: code-review\ndescription: Comprehensive code review\n---\n\nReview the following code for:\n- Security vulnerabilities\n- Performance issues\n- Best practices\n- Potential bugs\n\nCode:\n{input}".to_string(),
                        size_bytes: 512,
                    },
                ],
            },
        ),
        (
            "test/ai-tools",
            Package {
                metadata: PackageMetadata {
                    name: "test/ai-tools".to_string(),
                    version: "2.1.0".to_string(),
                    description: "AI-powered development prompts".to_string(),
                    author: "test-user".to_string(),
                    tags: vec!["ai".to_string(), "development".to_string()],
                    license: "MIT".to_string(),
                    created_at: "2025-01-02T00:00:00Z".to_string(),
                    updated_at: "2025-01-03T00:00:00Z".to_string(),
                    downloads: 128,
                    size_bytes: 4096,
                },
                prompts: vec![
                    PackagePrompt {
                        name: "explain-code".to_string(),
                        content: "---\nid: explain-code\ndescription: Explain complex code clearly\n---\n\nExplain this code in simple terms:\n\n{input}\n\nInclude:\n- Purpose\n- How it works\n- Key concepts\n- Usage examples".to_string(),
                        size_bytes: 512,
                    },
                ],
            },
        ),
    ];

    // Find matching package
    for (name, package) in packages {
        if name == package_name {
            return Ok(HttpResponse::Ok().json(package));
        }
    }

    Ok(HttpResponse::NotFound().json(json!({
        "error": "Package not found"
    })))
}

async fn publish_package(package: web::Json<serde_json::Value>) -> Result<HttpResponse> {
    // Mock successful publish
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "package": {
            "name": package.get("name").and_then(|v| v.as_str()).unwrap_or("unknown"),
            "version": package.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0"),
            "description": package.get("description").and_then(|v| v.as_str()).unwrap_or(""),
            "author": "test-user",
            "tags": [],
            "license": "MIT",
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z",
            "downloads": 0,
            "size_bytes": 1024
        }
    })))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("🚀 Starting PromptHive Mock Registry on http://localhost:8080");
    println!("📦 Available test packages:");
    println!("   - test/essentials (productivity prompts)");
    println!("   - test/ai-tools (AI development prompts)");
    println!("   - team/shared (collaboration prompts)");
    println!();
    println!("🔍 Test with:");
    println!("   PROMPTHIVE_REGISTRY_URL=http://localhost:8080 ph browse ai");
    println!("   PROMPTHIVE_REGISTRY_URL=http://localhost:8080 ph install test/essentials");

    HttpServer::new(|| {
        App::new()
            .route("/api/v1/search", web::get().to(search_packages))
            .route("/api/v1/packages/{name:.*}", web::get().to(get_package))
            .route("/api/v1/packages", web::post().to(publish_package))
            .route(
                "/",
                web::get()
                    .to(|| async { HttpResponse::Ok().body("PromptHive Mock Registry v1.0") }),
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
