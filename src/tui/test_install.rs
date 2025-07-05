#[cfg(test)]
mod tests {
    use crate::storage::Storage;
    use crate::tui::{Bank, BankType, InstallOptions, InstallPreview, InstallStage, Prompt};

    #[test]
    fn test_install_workflow_preview_stage() {
        let _storage = Storage::new().unwrap();

        // Create a mock bank for testing
        let test_bank = Bank {
            name: "test-bank".to_string(),
            display_name: "Test Bank".to_string(),
            description: "A test bank for installation".to_string(),
            author: "Test Author".to_string(),
            version: "1.0.0".to_string(),
            tags: vec!["test".to_string()],
            prompts: vec![
                Prompt {
                    name: "prompt1".to_string(),
                    description: "First test prompt".to_string(),
                    content: "Content of prompt 1".to_string(),
                    bank_name: Some("test-bank".to_string()),
                    created_at: Some("2024-01-01T00:00:00Z".to_string()),
                    updated_at: None,
                    tags: vec![],
                    is_favorite: false,
                    usage_count: 0,
                },
                Prompt {
                    name: "prompt2".to_string(),
                    description: "Second test prompt".to_string(),
                    content: "Content of prompt 2".to_string(),
                    bank_name: Some("test-bank".to_string()),
                    created_at: Some("2024-01-01T00:00:00Z".to_string()),
                    updated_at: None,
                    tags: vec![],
                    is_favorite: false,
                    usage_count: 0,
                },
            ],
            bank_type: BankType::Registry,
            is_expanded: false,
        };

        // Create install preview
        let mut preview = InstallPreview {
            bank: test_bank.clone(),
            selected_prompts: test_bank.prompts.iter().map(|p| p.name.clone()).collect(),
            install_options: InstallOptions {
                install_all: true,
                create_bank: true,
                merge_with_existing: false,
            },
            stage: InstallStage::Preview,
            cursor_index: 0,
        };

        // Test initial state
        assert_eq!(preview.stage, InstallStage::Preview);
        assert_eq!(preview.selected_prompts.len(), 2);
        assert!(preview.install_options.install_all);
        assert!(preview.install_options.create_bank);
        assert!(!preview.install_options.merge_with_existing);

        // Test toggling install all
        preview.install_options.install_all = false;
        preview.selected_prompts.clear();
        assert_eq!(preview.selected_prompts.len(), 0);

        // Test individual selection
        preview.selected_prompts.insert("prompt1".to_string());
        assert_eq!(preview.selected_prompts.len(), 1);
        assert!(preview.selected_prompts.contains("prompt1"));

        // Test stage progression
        preview.stage = InstallStage::Confirm;
        assert_eq!(preview.stage, InstallStage::Confirm);
    }

    #[test]
    fn test_install_workflow_installation() {
        use tempfile::TempDir;

        // Create a temporary directory for test storage
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("PROMPTHIVE_BASE_DIR", temp_dir.path());

        let storage = Storage::new().unwrap();

        // Ensure the bank directory exists
        std::fs::create_dir_all(temp_dir.path().join("banks/test-bank")).unwrap();

        // Create test prompts to install
        let metadata1 = crate::storage::PromptMetadata {
            id: "test1".to_string(),
            description: "Test prompt 1".to_string(),
            tags: Some(vec!["test".to_string()]),
            created_at: Some("2024-01-01T00:00:00Z".to_string()),
            updated_at: None,
            version: None,
            git_hash: None,
            parent_version: None,
        };

        let metadata2 = crate::storage::PromptMetadata {
            id: "test2".to_string(),
            description: "Test prompt 2".to_string(),
            tags: None,
            created_at: Some("2024-01-01T00:00:00Z".to_string()),
            updated_at: None,
            version: None,
            git_hash: None,
            parent_version: None,
        };

        // Test installing to a bank
        storage
            .write_prompt("test-bank/test1", &metadata1, "Content 1")
            .unwrap();
        storage
            .write_prompt("test-bank/test2", &metadata2, "Content 2")
            .unwrap();

        assert!(storage.prompt_exists("test-bank/test1"));
        assert!(storage.prompt_exists("test-bank/test2"));

        // Test reading back
        let (read_meta, content) = storage.read_prompt("test-bank/test1").unwrap();
        assert_eq!(read_meta.id, "test1");
        assert_eq!(content, "Content 1");
    }

    #[test]
    fn test_install_conflict_handling() {
        use tempfile::TempDir;

        // Create a temporary directory for test storage
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("PROMPTHIVE_BASE_DIR", temp_dir.path());

        let storage = Storage::new().unwrap();

        // Ensure the prompts directory exists
        std::fs::create_dir_all(temp_dir.path().join("prompts")).unwrap();

        let metadata = crate::storage::PromptMetadata {
            id: "existing".to_string(),
            description: "Existing prompt".to_string(),
            tags: None,
            created_at: Some("2024-01-01T00:00:00Z".to_string()),
            updated_at: None,
            version: None,
            git_hash: None,
            parent_version: None,
        };

        // Create an existing prompt
        storage
            .write_prompt("existing", &metadata, "Original content")
            .unwrap();
        assert!(storage.prompt_exists("existing"));

        // Test that we can detect the conflict
        assert!(storage.prompt_exists("existing"));

        // Test auto-rename logic
        let mut counter = 1;
        let mut unique_name = format!("existing-{}", counter);
        while storage.prompt_exists(&unique_name) {
            counter += 1;
            unique_name = format!("existing-{}", counter);
        }

        assert_eq!(unique_name, "existing-1");

        // Write with renamed version
        storage
            .write_prompt(&unique_name, &metadata, "New content")
            .unwrap();
        assert!(storage.prompt_exists("existing-1"));

        // Original should still exist
        assert!(storage.prompt_exists("existing"));
    }

    #[test]
    fn test_install_options_behavior() {
        let options = InstallOptions {
            install_all: true,
            create_bank: false,
            merge_with_existing: true,
        };

        assert!(options.install_all);
        assert!(!options.create_bank);
        assert!(options.merge_with_existing);
    }
}
