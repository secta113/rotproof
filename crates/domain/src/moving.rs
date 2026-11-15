//! The code that is moving into the layers: which files to move first.
//!
//! The paths a project lists in `moving` hold code that is not in a layer yet, which no check judges. `rotproof check`
//! counts it and says what to move next, so that the count falls only when a file is moved.
//!
//! - **The order is the import graph's, from its leaves.** A file imports files that are moving too, and cannot go
//!   into a layer before what it imports is where it belongs: the onion is built from the inside. A file that imports
//!   no other file still moving comes first. A file that imports one still moving waits for it, and an import of a
//!   file in a layer or of a package outside the project is no wait.
//! - **Files that import each other are one group.** Neither goes first: they are moved together, and shown together.
//! - **No date is set,** and nothing fails: the count and the next files stay as long as the files do.
//!
//! The graph is built by `application`, which reads the code and resolves each import to a file; the rules here take
//! it as plain values.

use std::collections::{BTreeMap, BTreeSet};

/// The most groups `rotproof check` names as next to move: the rest are counted
pub const SHOWN: usize = 5;

/// A file that is moving, and the files that are moving too that it imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// From the root, with `/`
    pub path: String,
    /// The files it imports among those that are moving, never itself
    pub needs: BTreeSet<String>,
}

/// What `rotproof check` says of the code that is moving.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Order {
    /// How many files are moving
    pub total: usize,
    /// Whether the files were put in an order: false for a language whose files' imports are not read
    pub ordered: bool,
    /// The groups to move next, the first [`SHOWN`] of those that wait for no other file: one file alone, or the files
    /// that import each other, each in the byte order of its path, and the groups in the byte order of their first
    pub next: Vec<Vec<String>>,
    /// How many files are in the groups that wait for no other file and are not in `next`
    pub more: usize,
}

impl Order {
    /// Files that are counted and not ordered.
    pub fn unordered(total: usize) -> Order {
        Order {
            total,
            ..Order::default()
        }
    }
}

/// The files of `files` put in order: the groups that wait for no file still moving come first, and a file or group
/// that waits for another comes only once that one is gone.
pub fn order(files: &[File]) -> Order {
    let groups = components(files);
    let needs: BTreeMap<&str, &BTreeSet<String>> = files
        .iter()
        .map(|file| (file.path.as_str(), &file.needs))
        .collect();
    // A group waits for no other when everything its files import is in the group, or is no file still moving
    let ready: Vec<&Vec<String>> = groups
        .iter()
        .filter(|group| {
            group.iter().all(|path| {
                needs[path.as_str()].iter().all(|need| {
                    group.binary_search(need).is_ok() || !needs.contains_key(need.as_str())
                })
            })
        })
        .collect();
    let next: Vec<Vec<String>> = ready
        .iter()
        .take(SHOWN)
        .map(|group| (*group).clone())
        .collect();
    let more = ready.iter().skip(SHOWN).map(|group| group.len()).sum();
    Order {
        total: files.len(),
        ordered: true,
        next,
        more,
    }
}

/// The groups of files that import each other, each file in one: a file that imports no file that imports it is a group
/// of its own. Each group is in the byte order of its paths, and the groups in the byte order of their first path.
fn components(files: &[File]) -> Vec<Vec<String>> {
    let index: BTreeMap<&str, usize> = files
        .iter()
        .enumerate()
        .map(|(i, file)| (file.path.as_str(), i))
        .collect();
    let needs: Vec<Vec<usize>> = files
        .iter()
        .map(|file| {
            file.needs
                .iter()
                .filter_map(|need| index.get(need.as_str()).copied())
                .collect()
        })
        .collect();
    // Tarjan's algorithm, with its own stack of calls so that a long chain of imports cannot overflow the real one
    let unseen = usize::MAX;
    let mut number = vec![unseen; files.len()];
    let mut low = vec![0; files.len()];
    let mut open = vec![false; files.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut found: Vec<Vec<String>> = Vec::new();
    let mut counter = 0;
    for root in 0..files.len() {
        if number[root] != unseen {
            continue;
        }
        let mut calls: Vec<(usize, usize)> = vec![(root, 0)];
        number[root] = counter;
        low[root] = counter;
        counter += 1;
        stack.push(root);
        open[root] = true;
        while let Some(&(at, next)) = calls.last() {
            if let Some(&to) = needs[at].get(next) {
                if let Some(call) = calls.last_mut() {
                    call.1 += 1;
                }
                if number[to] == unseen {
                    number[to] = counter;
                    low[to] = counter;
                    counter += 1;
                    stack.push(to);
                    open[to] = true;
                    calls.push((to, 0));
                } else if open[to] {
                    low[at] = low[at].min(number[to]);
                }
                continue;
            }
            calls.pop();
            if let Some(&(parent, _)) = calls.last() {
                low[parent] = low[parent].min(low[at]);
            }
            if low[at] == number[at] {
                let mut group = Vec::new();
                while let Some(top) = stack.pop() {
                    open[top] = false;
                    group.push(files[top].path.clone());
                    if top == at {
                        break;
                    }
                }
                group.sort();
                found.push(group);
            }
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, needs: &[&str]) -> File {
        File {
            path: path.to_string(),
            needs: needs.iter().map(|need| need.to_string()).collect(),
        }
    }

    fn groups(order: &Order) -> Vec<Vec<&str>> {
        order
            .next
            .iter()
            .map(|group| group.iter().map(String::as_str).collect())
            .collect()
    }

    #[test]
    fn the_files_that_import_no_file_still_moving_come_first() {
        let order = order(&[
            file("a.py", &["b.py"]),
            file("b.py", &["c.py"]),
            file("c.py", &[]),
            file("d.py", &[]),
            file("e.py", &["a.py"]),
        ]);
        assert_eq!(order.total, 5);
        assert!(order.ordered);
        assert_eq!(groups(&order), [["c.py"], ["d.py"]]);
        assert_eq!(order.more, 0);
    }

    #[test]
    fn files_that_import_each_other_are_one_group_that_waits_for_what_it_imports_outside() {
        let files = [
            file("a.py", &["b.py"]),
            file("b.py", &["c.py", "a.py"]),
            file("c.py", &["b.py"]),
            file("d.py", &["a.py"]),
            file("e.py", &["x.py"]),
            file("x.py", &["y.py"]),
            file("y.py", &["x.py"]),
        ];
        let order = order(&files);
        // a, b and c are one group of three that imports nothing outside it; d waits for it; e waits for x and y, which
        // import each other and nothing else
        assert_eq!(
            groups(&order),
            [vec!["a.py", "b.py", "c.py"], vec!["x.py", "y.py"]]
        );
    }

    #[test]
    fn a_group_that_waits_for_another_is_not_next() {
        let order = order(&[
            file("a.py", &["b.py"]),
            file("b.py", &["a.py", "c.py"]),
            file("c.py", &[]),
        ]);
        assert_eq!(groups(&order), [["c.py"]]);
    }

    #[test]
    fn a_file_that_imports_itself_is_a_file_alone() {
        let order = order(&[file("a.py", &["a.py"])]);
        assert_eq!(groups(&order), [["a.py"]]);
    }

    #[test]
    fn only_the_first_groups_are_named_and_the_files_of_the_rest_are_counted() {
        let files: Vec<File> = (0..8)
            .map(|i| file(&format!("m{i}.py"), &[]))
            .chain([file("x.py", &["y.py"]), file("y.py", &["x.py"])])
            .collect();
        let order = order(&files);
        assert_eq!(order.total, 10);
        assert_eq!(order.next.len(), SHOWN);
        // Three single files and a group of two: the groups are in the byte order of their first path
        assert_eq!(order.more, 5);
    }

    #[test]
    fn a_long_chain_does_not_overflow_the_stack() {
        let count = 100_000;
        let files: Vec<File> = (0..count)
            .map(|i| File {
                path: format!("f{i:06}.py"),
                needs: (i + 1 < count)
                    .then(|| format!("f{:06}.py", i + 1))
                    .into_iter()
                    .collect(),
            })
            .collect();
        let order = order(&files);
        assert_eq!(order.total, count);
        assert_eq!(groups(&order), [[format!("f{:06}.py", count - 1).as_str()]]);
    }
}
