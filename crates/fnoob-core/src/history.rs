use crate::model::Project;

pub const UNDO_LIMIT: usize = 100;

#[derive(Default)]
pub struct History {
    undo: Vec<Project>,
    redo: Vec<Project>,
}

impl History {
    pub fn record(&mut self, before: Project) {
        if self.undo.len() == UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push(before);
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &mut Project) -> bool {
        match self.undo.pop() {
            Some(previous) => {
                self.redo.push(std::mem::replace(current, previous));
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self, current: &mut Project) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(current, next));
                true
            }
            None => false,
        }
    }

    pub fn snapshots(&self) -> impl Iterator<Item = &Project> {
        self.undo.iter().chain(self.redo.iter())
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> Project {
        Project::new(name, 48_000)
    }

    #[test]
    fn undo_and_redo_restore_states() {
        let mut h = History::default();
        let mut current = named("b");
        h.record(named("a"));
        assert!(h.undo(&mut current));
        assert_eq!(current.name, "a");
        assert!(!h.undo(&mut current));
        assert!(h.redo(&mut current));
        assert_eq!(current.name, "b");
        assert!(!h.redo(&mut current));
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut h = History::default();
        let mut current = named("b");
        h.record(named("a"));
        h.undo(&mut current);
        h.record(named("c"));
        assert!(!h.redo(&mut current));
    }

    #[test]
    fn limit_drops_oldest() {
        let mut h = History::default();
        let mut current = named("now");
        for i in 0..UNDO_LIMIT + 5 {
            h.record(named(&i.to_string()));
        }
        let mut steps = 0;
        while h.undo(&mut current) {
            steps += 1;
        }
        assert_eq!(steps, UNDO_LIMIT);
        assert_eq!(current.name, "5");
    }
}
