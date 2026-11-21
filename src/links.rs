//! Transitive ID links, named by the byte-wise smallest member.
type HashMap<K, V> = std::collections::HashMap<K, V, ahash::RandomState>;

#[derive(Default)]
pub struct Links {
    ids: HashMap<String, usize>,
    parents: Vec<usize>,
    sizes: Vec<usize>,
    names: Vec<String>,
}
impl Links {
    fn intern(&mut self, id: &str) -> usize {
        if let Some(&index) = self.ids.get(id) { return index; }
        let index = self.parents.len();
        self.ids.insert(id.to_owned(), index);
        self.parents.push(index);
        self.sizes.push(1);
        self.names.push(id.to_owned());
        index
    }
    fn root(&mut self, mut index: usize) -> usize {
        while self.parents[index] != index {
            self.parents[index] = self.parents[self.parents[index]];
            index = self.parents[index];
        }
        index
    }
    /// Empty IDs are invalid and do not create links.
    pub fn join(&mut self, a: &str, b: &str) {
        if a.is_empty() || b.is_empty() { return; }
        let ai = self.intern(a); let bi = self.intern(b);
        let mut a = self.root(ai); let mut b = self.root(bi);
        if a == b { return; }
        if self.sizes[a] < self.sizes[b] { std::mem::swap(&mut a, &mut b); }
        self.parents[b] = a;
        self.sizes[a] += self.sizes[b];
        if self.names[b] < self.names[a] { self.names[a] = self.names[b].clone(); }
    }
    /// Resolve a name without mutating the union-find; safe to call concurrently.
    pub fn name_shared<'a>(&'a self, id: &'a str) -> &'a str {
        let Some(&mut_index) = self.ids.get(id) else { return id; };
        let mut index = mut_index;
        while self.parents[index] != index { index = self.parents[index]; }
        &self.names[index]
    }
    /// Unlinked IDs name themselves.
    pub fn name<'a>(&'a mut self, id: &'a str) -> &'a str {
        match self.ids.get(id).copied() {
            Some(index) => { let root = self.root(index); &self.names[root] }
            None => id,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn transitive_and_names() {
        let mut links = Links::default();
        links.join("z", "y"); links.join("x", "y"); links.join("A", "x");
        links.join("", "0"); links.join("z", "z");
        assert_eq!(links.name_shared("z"), "A");
        assert_eq!(links.name_shared("unlinked"), "unlinked");
        assert_eq!(links.name("z"), "A");
        assert_eq!(links.name("x"), "A");
        assert_eq!(links.name("0"), "0");
        assert_eq!(links.name("unlinked"), "unlinked");
    }
}
