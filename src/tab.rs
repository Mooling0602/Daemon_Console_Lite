//! Tab completion system with the context-aware completion tree.
//!
//! This module provides a flexible completion system that allows registering
//! completion items based on input context, without requiring strict prefix matching.

/// Matching strategy for filtering completion candidates.
#[derive(Clone, Default)]
pub enum MatchStrategy {
    /// Show all completions regardless of the current input
    All,
    /// Match completions that start with the current input suffix
    #[default]
    Prefix,
    /// Match completions that contain the current input suffix
    Contains,
}

/// A single completion item with text and optional description.
#[derive(Clone, Debug)]
pub struct CompletionItem {
    /// The text to complete
    pub text: String,
    /// Optional description for display
    pub description: Option<String>,
    /// Priority for sorting (higher = more important)
    pub priority: u32,
}

impl CompletionItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            description: None,
            priority: 0,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }
}

/// A node in the completion tree representing a context state.
pub struct TabNode {
    /// Trigger pattern that activates this node (None for root)
    trigger: Option<String>,
    /// Completions available in this context
    completions: Vec<CompletionItem>,
    /// Child nodes for deeper contexts
    children: Vec<TabNode>,
    /// Strategy for matching completions
    match_strategy: MatchStrategy,
}

impl TabNode {
    fn new(trigger: Option<String>) -> Self {
        Self {
            trigger,
            completions: Vec::new(),
            children: Vec::new(),
            match_strategy: MatchStrategy::default(),
        }
    }

    fn root() -> Self {
        Self::new(None)
    }
}

/// Completion candidate ready for display/use.
#[derive(Clone, Debug)]
pub struct CompletionCandidate {
    /// The full completed text
    pub full_text: String,
    /// Just the completion part (without context)
    pub completion: String,
    /// Optional description
    pub description: Option<String>,
}

/// Tab completion tree manager.
pub struct TabTree {
    root: TabNode,
    /// Cache of current candidates
    current_candidates: Vec<CompletionCandidate>,
    /// Last input for cache invalidation
    last_input: String,
}

impl TabTree {
    /// Creates a new empty tab completion tree.
    pub fn new() -> Self {
        Self {
            root: TabNode::root(),
            current_candidates: Vec::new(),
            last_input: String::new(),
        }
    }

    /// Registers completions for a given context.
    ///
    /// # Arguments
    ///
    /// * `context` - The input prefix that triggers these completions (empty string for root)
    /// * `completions` - List of completion texts
    ///
    /// # Examples
    ///
    /// ```
    /// use daemon_console_lite::tab::TabTree;
    ///
    /// let mut tree = TabTree::new();
    /// tree.register_completions("!config", &["start", "stop", "restart"]);
    /// ```
    pub fn register_completions(&mut self, context: &str, completions: &[&str]) {
        let items: Vec<CompletionItem> = completions
            .iter()
            .map(|&text| CompletionItem::new(text))
            .collect();
        self.register_completions_advanced(context, items, MatchStrategy::default());
    }

    /// Registers completions with descriptions.
    ///
    /// # Arguments
    ///
    /// * `context` - The input prefix that triggers these completions
    /// * `items` - List of (text, description) tuples
    pub fn register_completions_with_desc(&mut self, context: &str, items: &[(&str, &str)]) {
        let completion_items: Vec<CompletionItem> = items
            .iter()
            .map(|&(text, desc)| CompletionItem::new(text).with_description(desc))
            .collect();
        self.register_completions_advanced(context, completion_items, MatchStrategy::default());
    }

    /// Registers completions with the custom match strategy.
    ///
    /// # Arguments
    ///
    /// * `context` - The input prefix that triggers these completions
    /// * `items` - List of completion items
    /// * `strategy` - Matching strategy to use
    pub fn register_completions_advanced(
        &mut self,
        context: &str,
        items: Vec<CompletionItem>,
        strategy: MatchStrategy,
    ) {
        let trigger = if context.is_empty() {
            None
        } else {
            Some(context.to_string())
        };

        // Find or create the node
        if let Some(node) = self.find_or_create_node(trigger.as_deref()) {
            node.completions.extend(items);
            node.match_strategy = strategy;
        }
    }

    /// Adds a single completion item to an existing context.
    ///
    /// # Arguments
    ///
    /// * `context` - The context to add to
    /// * `text` - Completion text
    /// * `description` - Optional description
    pub fn add_completion(&mut self, context: &str, text: &str, description: Option<&str>) {
        let trigger = if context.is_empty() {
            None
        } else {
            Some(context.to_string())
        };

        if let Some(node) = self.find_or_create_node(trigger.as_deref()) {
            let mut item = CompletionItem::new(text);
            if let Some(desc) = description {
                item = item.with_description(desc);
            }
            node.completions.push(item);
        }
    }

    /// Finds or creates a node with the given trigger.
    fn find_or_create_node(&mut self, trigger: Option<&str>) -> Option<&mut TabNode> {
        if trigger.is_none() {
            return Some(&mut self.root);
        }

        let trigger_str = trigger.unwrap();

        // Try to find the existing node
        fn find_node_exists(node: &TabNode, trigger: &str) -> bool {
            if node.trigger.as_deref() == Some(trigger) {
                return true;
            }
            for child in &node.children {
                if find_node_exists(child, trigger) {
                    return true;
                }
            }
            false
        }

        // If the node doesn't exist, create it
        if !find_node_exists(&self.root, trigger_str) {
            let new_node = TabNode::new(Some(trigger_str.to_string()));
            self.root.children.push(new_node);
        }

        // Now find and return a mutable reference
        fn find_node_mut<'a>(node: &'a mut TabNode, trigger: &str) -> Option<&'a mut TabNode> {
            if node.trigger.as_deref() == Some(trigger) {
                return Some(node);
            }
            for child in &mut node.children {
                if let Some(found) = find_node_mut(child, trigger) {
                    return Some(found);
                }
            }
            None
        }

        find_node_mut(&mut self.root, trigger_str)
    }

    /// Finds the deepest matching node for the given input.
    fn find_deepest_match(&self, input: &str) -> &TabNode {
        let mut best_match = &self.root;
        let mut best_match_len = 0;

        fn search<'a>(
            node: &'a TabNode,
            input: &str,
            best: &mut &'a TabNode,
            best_len: &mut usize,
        ) {
            if let Some(trigger) = &node.trigger
                && input.starts_with(trigger)
                && trigger.len() > *best_len
            {
                *best = node;
                *best_len = trigger.len();
            }

            for child in &node.children {
                search(child, input, best, best_len);
            }
        }

        search(&self.root, input, &mut best_match, &mut best_match_len);
        best_match
    }

    /// Gets completion candidates for the current input.
    ///
    /// # Arguments
    ///
    /// * `input` - Current user input
    ///
    /// # Returns
    ///
    /// List of completion candidates, sorted by priority
    pub fn get_candidates(&mut self, input: &str) -> Vec<CompletionCandidate> {
        // Use cache if the input hasn't changed
        if input == self.last_input {
            return self.current_candidates.clone();
        }

        self.last_input = input.to_string();

        // Find the deepest matching node
        let node = self.find_deepest_match(input);

        // Get completions from the node
        let mut candidates = node.completions.clone();

        // Apply match strategy
        match &node.match_strategy {
            MatchStrategy::All => {
                // Don't filter, show all
            }
            MatchStrategy::Prefix => {
                // Get the part of input after the trigger
                let suffix = if let Some(trigger) = &node.trigger {
                    input
                        .strip_prefix(trigger.as_str())
                        .unwrap_or("")
                        .trim_start()
                } else {
                    input
                };

                if !suffix.is_empty() {
                    candidates.retain(|item| item.text.starts_with(suffix));
                }
            }
            MatchStrategy::Contains => {
                let search = input.split_whitespace().last().unwrap_or("");
                if !search.is_empty() {
                    candidates.retain(|item| item.text.contains(search));
                }
            }
        }

        // Sort by priority (higher first)
        candidates.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Build completion candidates
        let trigger_prefix = node.trigger.as_deref().unwrap_or("");
        let result: Vec<CompletionCandidate> = candidates
            .into_iter()
            .map(|item| {
                let full_text = if trigger_prefix.is_empty() {
                    item.text.clone()
                } else {
                    format!("{} {}", trigger_prefix, item.text)
                };

                CompletionCandidate {
                    full_text,
                    completion: item.text,
                    description: item.description,
                }
            })
            .collect();

        self.current_candidates = result.clone();
        result
    }

    /// Gets the best match (first candidate) for the given input.
    pub fn get_best_match(&mut self, input: &str) -> Option<String> {
        let candidates = self.get_candidates(input);
        candidates.first().map(|c| c.full_text.clone())
    }

    /// Clears the candidate cache.
    pub fn clear_cache(&mut self) {
        self.last_input.clear();
        self.current_candidates.clear();
    }
}

impl Default for TabTree {
    fn default() -> Self {
        Self::new()
    }
}
