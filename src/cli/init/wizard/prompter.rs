use anyhow::Result;
use dialoguer::{Confirm, Input, MultiSelect, Select};

/// Trait for interactive prompts (enables testing via mocks)
pub trait Prompter: Send + Sync {
    /// Display a single-select menu and return the selected index
    fn select(&self, prompt: &str, options: &[&str], default: usize) -> Result<usize>;

    /// Display a multi-select menu and return the selected indices
    fn multi_select(&self, prompt: &str, options: &[&str]) -> Result<Vec<usize>>;

    /// Prompt for text input with an optional default value
    fn input(&self, prompt: &str, default: Option<&str>) -> Result<String>;

    /// Prompt for yes/no confirmation
    fn confirm(&self, prompt: &str, default: bool) -> Result<bool>;
}

/// Real implementation using the dialoguer crate
pub struct DialoguerPrompter;

impl Prompter for DialoguerPrompter {
    fn select(&self, prompt: &str, options: &[&str], default: usize) -> Result<usize> {
        let selection = Select::new()
            .with_prompt(prompt)
            .items(options)
            .default(default)
            .interact()?;

        Ok(selection)
    }

    fn multi_select(&self, prompt: &str, options: &[&str]) -> Result<Vec<usize>> {
        let selections = MultiSelect::new()
            .with_prompt(prompt)
            .items(options)
            .interact()?;

        Ok(selections)
    }

    fn input(&self, prompt: &str, default: Option<&str>) -> Result<String> {
        let mut input = Input::<String>::new().with_prompt(prompt);

        if let Some(default_value) = default {
            input = input.default(default_value.to_string());
        }

        let value = input.interact_text()?;
        Ok(value)
    }

    fn confirm(&self, prompt: &str, default: bool) -> Result<bool> {
        let confirmed = Confirm::new()
            .with_prompt(prompt)
            .default(default)
            .interact()?;

        Ok(confirmed)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Mock prompter for testing
    pub struct MockPrompter {
        select_responses: Mutex<Vec<usize>>,
        multi_select_responses: Mutex<Vec<Vec<usize>>>,
        input_responses: Mutex<Vec<String>>,
        confirm_responses: Mutex<Vec<bool>>,
    }

    impl MockPrompter {
        pub fn new() -> Self {
            Self {
                select_responses: Mutex::new(Vec::new()),
                multi_select_responses: Mutex::new(Vec::new()),
                input_responses: Mutex::new(Vec::new()),
                confirm_responses: Mutex::new(Vec::new()),
            }
        }

        pub fn with_select_responses(self, responses: Vec<usize>) -> Self {
            *self.select_responses.lock().unwrap() = responses;
            self
        }

        pub fn with_multi_select_responses(self, responses: Vec<Vec<usize>>) -> Self {
            *self.multi_select_responses.lock().unwrap() = responses;
            self
        }

        pub fn with_input_responses(self, responses: Vec<String>) -> Self {
            *self.input_responses.lock().unwrap() = responses;
            self
        }

        pub fn with_confirm_responses(self, responses: Vec<bool>) -> Self {
            *self.confirm_responses.lock().unwrap() = responses;
            self
        }
    }

    impl Prompter for MockPrompter {
        fn select(&self, _prompt: &str, _options: &[&str], default: usize) -> Result<usize> {
            let mut responses = self.select_responses.lock().unwrap();

            if responses.is_empty() {
                Ok(default)
            } else {
                Ok(responses.remove(0))
            }
        }

        fn multi_select(&self, _prompt: &str, _options: &[&str]) -> Result<Vec<usize>> {
            let mut responses = self.multi_select_responses.lock().unwrap();

            if responses.is_empty() {
                Ok(vec![0])
            } else {
                Ok(responses.remove(0))
            }
        }

        fn input(&self, _prompt: &str, default: Option<&str>) -> Result<String> {
            let mut responses = self.input_responses.lock().unwrap();

            if responses.is_empty() {
                Ok(default.unwrap_or("").to_string())
            } else {
                Ok(responses.remove(0))
            }
        }

        fn confirm(&self, _prompt: &str, default: bool) -> Result<bool> {
            let mut responses = self.confirm_responses.lock().unwrap();

            if responses.is_empty() {
                Ok(default)
            } else {
                Ok(responses.remove(0))
            }
        }
    }
}
