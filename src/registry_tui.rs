use crate::registry::{Package, RegistryClient as BaseRegistryClient};
use crate::tui::{Bank, BankType, Prompt};

/// TUI-specific registry client that wraps the base registry client
pub struct TuiRegistryClient {
    #[allow(dead_code)]
    base_client: BaseRegistryClient,
}

impl TuiRegistryClient {
    /// Create a new TUI registry client
    pub fn new(base_url: Option<String>) -> Self {
        let url = base_url.unwrap_or_else(crate::registry::default_registry_url);
        Self {
            base_client: BaseRegistryClient::new(url),
        }
    }

    /// Convert a registry package to a TUI bank
    pub fn package_to_bank(&self, package: &Package) -> Bank {
        Bank {
            name: package.metadata.name.clone(),
            display_name: self.format_display_name(&package.metadata.name),
            description: package.metadata.description.clone(),
            author: package.metadata.author.clone(),
            version: package.metadata.version.clone(),
            tags: package.metadata.tags.clone(),
            prompts: package
                .prompts
                .iter()
                .map(|p| Prompt {
                    name: p.name.clone(),
                    description: self.extract_description(&p.content),
                    content: self.extract_body(&p.content),
                    bank_name: Some(package.metadata.name.clone()),
                    created_at: Some(package.metadata.created_at.clone()),
                    updated_at: Some(package.metadata.updated_at.clone()),
                    tags: vec![],
                    is_favorite: false,
                    usage_count: 0,
                })
                .collect(),
            bank_type: BankType::Registry,
            is_expanded: false,
        }
    }

    /// Get mock banks for demo purposes (until real registry is available)
    pub fn get_demo_banks(&self) -> Vec<Bank> {
        vec![
            Bank {
                name: "@featured/ai-assistants".to_string(),
                display_name: "AI Assistants Collection".to_string(),
                description: "Professional AI assistant prompts for various tasks".to_string(),
                author: "PromptHive Team".to_string(),
                version: "2.1.0".to_string(),
                tags: vec!["featured".to_string(), "ai".to_string(), "assistant".to_string()],
                prompts: vec![
                    Prompt {
                        name: "code-reviewer".to_string(),
                        description: "AI code review assistant".to_string(),
                        content: "You are an expert code reviewer. Analyze the following code for:\n\n1. Code quality and best practices\n2. Potential bugs or issues\n3. Performance improvements\n4. Security vulnerabilities\n5. Suggestions for improvement\n\nProvide constructive feedback with specific examples.".to_string(),
                        bank_name: Some("@featured/ai-assistants".to_string()),
                        created_at: Some("2024-01-15T10:00:00Z".to_string()),
                        updated_at: Some("2024-01-20T15:30:00Z".to_string()),
                        tags: vec!["code".to_string(), "review".to_string()],
                        is_favorite: false,
                        usage_count: 0,
                    },
                    Prompt {
                        name: "documentation-writer".to_string(),
                        description: "Technical documentation assistant".to_string(),
                        content: "You are a technical writing expert. Create clear, comprehensive documentation for:\n\n{{content}}\n\nInclude:\n- Overview and purpose\n- Key features\n- Usage examples\n- API reference (if applicable)\n- Common issues and solutions".to_string(),
                        bank_name: Some("@featured/ai-assistants".to_string()),
                        created_at: Some("2024-01-15T10:00:00Z".to_string()),
                        updated_at: Some("2024-01-18T12:00:00Z".to_string()),
                        tags: vec!["documentation".to_string(), "writing".to_string()],
                        is_favorite: false,
                        usage_count: 0,
                    },
                ],
                bank_type: BankType::Registry,
                is_expanded: false,
            },
            Bank {
                name: "@trending/productivity".to_string(),
                display_name: "Productivity Boosters".to_string(),
                description: "Popular prompts for enhancing productivity and workflow".to_string(),
                author: "Community".to_string(),
                version: "1.5.2".to_string(),
                tags: vec!["trending".to_string(), "productivity".to_string(), "workflow".to_string()],
                prompts: vec![
                    Prompt {
                        name: "task-planner".to_string(),
                        description: "Smart task planning assistant".to_string(),
                        content: "Help me plan and organize my tasks for maximum productivity. Analyze my task list and:\n\n1. Prioritize based on importance and urgency\n2. Estimate time needed for each task\n3. Suggest optimal scheduling\n4. Identify potential bottlenecks\n5. Recommend batching similar tasks\n\nTask list:\n{{tasks}}".to_string(),
                        bank_name: Some("@trending/productivity".to_string()),
                        created_at: Some("2024-01-10T09:00:00Z".to_string()),
                        updated_at: Some("2024-01-22T14:15:00Z".to_string()),
                        tags: vec!["planning".to_string(), "tasks".to_string()],
                        is_favorite: false,
                        usage_count: 0,
                    },
                    Prompt {
                        name: "meeting-summarizer".to_string(),
                        description: "Extract key points from meeting notes".to_string(),
                        content: "Summarize the following meeting notes, extracting:\n\n1. Key decisions made\n2. Action items with assignees\n3. Important discussions\n4. Next steps\n5. Follow-up dates\n\nMeeting notes:\n{{notes}}".to_string(),
                        bank_name: Some("@trending/productivity".to_string()),
                        created_at: Some("2024-01-12T11:30:00Z".to_string()),
                        updated_at: Some("2024-01-19T16:45:00Z".to_string()),
                        tags: vec!["meetings".to_string(), "summary".to_string()],
                        is_favorite: false,
                        usage_count: 0,
                    },
                ],
                bank_type: BankType::Registry,
                is_expanded: false,
            },
            Bank {
                name: "@community/creative-writing".to_string(),
                display_name: "Creative Writing Toolkit".to_string(),
                description: "Inspiring prompts for creative writers and storytellers".to_string(),
                author: "WritersCollective".to_string(),
                version: "3.0.1".to_string(),
                tags: vec!["community".to_string(), "writing".to_string(), "creative".to_string()],
                prompts: vec![
                    Prompt {
                        name: "story-generator".to_string(),
                        description: "Generate creative story ideas".to_string(),
                        content: "Create a compelling story concept based on these elements:\n\nGenre: {{genre}}\nTheme: {{theme}}\nSetting: {{setting}}\n\nProvide:\n1. A unique premise\n2. Main character description\n3. Central conflict\n4. Three key plot points\n5. Potential ending".to_string(),
                        bank_name: Some("@community/creative-writing".to_string()),
                        created_at: Some("2024-01-08T14:00:00Z".to_string()),
                        updated_at: Some("2024-01-21T10:20:00Z".to_string()),
                        tags: vec!["story".to_string(), "generator".to_string()],
                        is_favorite: false,
                        usage_count: 0,
                    },
                ],
                bank_type: BankType::Registry,
                is_expanded: false,
            },
        ]
    }

    /// Extract description from prompt content (looks for frontmatter)
    fn extract_description(&self, content: &str) -> String {
        if content.starts_with("---") {
            let lines: Vec<&str> = content.lines().collect();
            for line in lines.iter().skip(1) {
                if line.starts_with("description:") {
                    return line.trim_start_matches("description:").trim().to_string();
                }
                if *line == "---" {
                    break;
                }
            }
        }
        "No description available".to_string()
    }

    /// Extract body content (skips frontmatter if present)
    fn extract_body(&self, content: &str) -> String {
        if content.starts_with("---") {
            let lines: Vec<&str> = content.lines().collect();
            if let Some(end_pos) = lines.iter().skip(1).position(|&line| line == "---") {
                return lines[(end_pos + 2)..].join("\n");
            }
        }
        content.to_string()
    }

    /// Format display name from package name
    fn format_display_name(&self, name: &str) -> String {
        // Convert @namespace/package-name to "Package Name"
        name.split('/')
            .next_back()
            .unwrap_or(name)
            .split('-')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect::<Vec<String>>()
            .join(" ")
    }
}
