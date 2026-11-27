pub trait Command<T> {
    fn apply(&mut self, target: &mut T);
    fn undo(&mut self, target: &mut T);
}

pub struct History<T> {
    applied: Vec<Box<dyn Command<T>>>,
    undone: Vec<Box<dyn Command<T>>>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            applied: Vec::new(),
            undone: Vec::new(),
        }
    }
}

impl<T> History<T> {
    pub fn execute(&mut self, mut command: impl Command<T> + 'static, target: &mut T) {
        command.apply(target);
        self.applied.push(Box::new(command));
        self.undone.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.applied.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    pub fn undo(&mut self, target: &mut T) {
        if let Some(mut command) = self.applied.pop() {
            command.undo(target);
            self.undone.push(command);
        }
    }

    pub fn redo(&mut self, target: &mut T) {
        if let Some(mut command) = self.undone.pop() {
            command.apply(target);
            self.applied.push(command);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Append(char);
    impl Command<String> for Append {
        fn apply(&mut self, target: &mut String) {
            target.push(self.0);
        }
        fn undo(&mut self, target: &mut String) {
            target.pop();
        }
    }

    #[test]
    fn undo_and_redo_follow_execution_order_until_a_new_edit() {
        let mut history = History::default();
        let mut value = String::new();
        history.undo(&mut value);
        history.redo(&mut value);
        assert!(!history.can_undo() && !history.can_redo());
        history.execute(Append('a'), &mut value);
        history.execute(Append('b'), &mut value);
        assert_eq!(value, "ab");
        history.undo(&mut value);
        history.undo(&mut value);
        assert!(value.is_empty());
        history.redo(&mut value);
        history.redo(&mut value);
        assert_eq!(value, "ab");
        history.undo(&mut value);
        history.execute(Append('c'), &mut value);
        history.redo(&mut value);
        assert_eq!(value, "ac");
        assert!(history.can_undo() && !history.can_redo());
    }
}
