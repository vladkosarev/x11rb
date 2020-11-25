use super::{Binding, Component, Entry};
use super::parser::parse_resource;

pub(crate) struct ZipLongest<A, B> {
    a: A,
    b: B,
}

impl<A, B> Iterator for ZipLongest<A, B>
where
    A: Iterator,
    B: Iterator,
{
    type Item = (Option<A::Item>, Option<B::Item>);

    fn next(&mut self) -> Option<Self::Item> {
        match (self.a.next(), self.b.next()) {
            (None, None) => None,
            (a, b) => Some((a, b)),
        }
    }
}

fn zip_longest<'a, T>(a: &'a [T], b: &'a [T]) -> impl Iterator<Item=(Option<&'a T>, Option<&'a T>)> + 'a {
    ZipLongest {
        a: a.iter(),
        b: b.iter(),
    }
}

#[cfg(test)]
mod test_zip_longest {
    use super::zip_longest;

    #[test]
    fn empty() {
        let (a, b): ([u8; 0], [u8; 0]) = ([], []);
        let res = zip_longest(&a, &b).collect::<Vec<_>>();
        assert_eq!(res, []);
    }

    #[test]
    fn same_length() {
        let a = [0, 1, 2];
        let b = [4, 5, 6];
        let expected = [
            (Some(&0), Some(&4)),
            (Some(&1), Some(&5)),
            (Some(&2), Some(&6)),
        ];
        let res = zip_longest(&a, &b).collect::<Vec<_>>();
        assert_eq!(res, expected);
    }

    #[test]
    fn first_shorter() {
        let a = [0, 1];
        let b = [4, 5, 6, 7];
        let expected = [
            (Some(&0), Some(&4)),
            (Some(&1), Some(&5)),
            (None, Some(&6)),
            (None, Some(&7)),
        ];
        let res = zip_longest(&a, &b).collect::<Vec<_>>();
        assert_eq!(res, expected);
    }

    #[test]
    fn second_shorter() {
        let a = [0, 1, 2, 3];
        let b = [4, 5];
        let expected = [
            (Some(&0), Some(&4)),
            (Some(&1), Some(&5)),
            (Some(&2), None),
            (Some(&3), None),
        ];
        let res = zip_longest(&a, &b).collect::<Vec<_>>();
        assert_eq!(res, expected);
    }
}

fn check_match(entry: &Entry, resource: &[String], class: &[String]) -> bool {
    // The idea is to check if a nondeterministic finite automaton accepts a given
    // word. We have a set of current states (indicies). This describes where in the
    // entry we are while trying to match. When we have a match, we go to the next
    // component in the entry (index + 1). When we have a loose binding, we can accept
    // the current component by staying in the same state (index).
    let mut indicies = vec![0];
    if let Some((Binding::Loose, _)) = entry.components.get(0) {
        // First binding is loose, so we could "start anywhere" with the match
        indicies.extend(1..entry.components.len());
    }
    // Go through the components and match them
    for (resource, class) in zip_longest(resource, class) {
        let mut next_indicies = Vec::new();
        for index in indicies {
            if index == entry.components.len() {
                // We are at the end of the entry and thus cannot continue this match
                continue;
            }
            match entry.components[index].0 {
                // We have to match here, no way around that.
                Binding::Tight => {}
                // We could "eat" this with the loose binding by staying in the state
                Binding::Loose => next_indicies.push(index)
            }
            // Does the component match?
            let matches = match entry.components[index].1 {
                Component::Wildcard => true,
                Component::Normal(ref s) => Some(s) == resource || Some(s) == class,
            };
            if matches {
                // Yes, the component matches and we go to the next state
                next_indicies.push(index + 1);
            }
        }
        indicies = next_indicies;
    }
    // We have a match if we reached the end of the components
    indicies.contains(&entry.components.len())
}

pub(crate) fn match_entry<'a>(database: &'a [Entry], resource: &str, class: &str) -> Option<&'a [u8]> {
    let resource = parse_resource(resource.as_bytes())?;
    let class = parse_resource(class.as_bytes())?;
    database.iter()
        .filter(|entry| check_match(entry, &resource, &class))
        .last()
        .map(|entry| &entry.value[..])
}

#[cfg(test)]
mod test {
    use super::super::parser::parse_database;
    use super::match_entry;

    // Most tests in here are based on [1], which is: Copyright © 2016 Ingo Bürk
    // [1]: https://github.com/Airblader/xcb-util-xrm/blob/master/tests/tests_match.c

    #[test]
    fn test_matches() {
        let tests = [
            // Non-matches / Errors
            (&b""[..], "", "", None),

            // Xlib returns the match here, despite the query violating the specs.
            (b"First.second: 1", "First.second", "First.second.third", None),
            (b"", "First.second", "", None),
            (b"First.second: 1", "First.third", "", None),
            (b"First.second: 1", "First", "", None),
            (b"First: 1", "First.second", "", None),
            (b"First.?.fourth: 1", "First.second.third.fourth", "", None),
            (b"First*?.third: 1", "First.third", "", None),
            (b"First: 1", "first", "", None),
            (b"First: 1", "", "first", None),

            // Duplicate entries
            (b"First: 1\nFirst: 2\nFirst: 3\n", "First", "", Some(&b"3"[..])),
            (b"First: 1\nSecond: 2\nSecond: 3\nThird: 4\n", "Second", "", Some(b"3")),

            /* Basic matching */
            (b"First: 1", "First", "", Some(b"1")),
            (b"First.second: 1", "First.second", "", Some(b"1")),
            (b"?.second: 1", "First.second", "", Some(b"1")),
            (b"First.?.third: 1", "First.second.third", "", Some(b"1")),
            (b"First.?.?.fourth: 1", "First.second.third.fourth", "", Some(b"1")),
            (b"*second: 1", "First.second", "", Some(b"1")),
            (b".second: 1", "First.second", "", None),
            (b"*third: 1", "First.second.third", "", Some(b"1")),
            (b"First*second: 1", "First.second", "", Some(b"1")),
            (b"First*third: 1", "First.second.third", "", Some(b"1")),
            (b"First*fourth: 1", "First.second.third.fourth", "", Some(b"1")),
            (b"First*?.third: 1", "First.second.third", "", Some(b"1")),
            (b"First: 1", "Second", "First", Some(b"1")),
            (b"First.second: 1", "First.third", "first.second", Some(b"1")),
            (b"First.second.third: 1", "First.third.third", "first.second.fourth", Some(b"1")),
            (b"First*third*fifth: 1", "First.second.third.fourth.third.fifth", "", Some(b"1")),
            (b"First: x\\\ny", "First", "", Some(b"xy")),
            (b"! First: x", "First", "", None),
            (b"# First: x", "First", "", None),
            (b"First:", "First", "", Some(b"")),
            (b"First: ", "First", "", Some(b"")),
            (b"First: \t ", "First", "", Some(b"")),

            // Consecutive bindings
            (b"*.bar: 1", "foo.foo.bar", "", Some(b"1")),
            (b"...bar: 1", "foo.bar", "", None),
            (b"...bar: 1", "foo.foo.foo.bar", "", None),
            (b"***bar: 1", "foo.bar", "", Some(b"1")),
            (b".*.bar: 1", "foo.bar", "", Some(b"1")),
            (b".*.bar: 1", "foo.foo.bar", "", Some(b"1")),
            (b"..*bar: 1", "foo.foo.foo.foo.bar", "", Some(b"1")),
            (b"a.*.z: 1", "a.b.c.d.e.f.z", "", Some(b"1")),
            (b"a...z: 1", "a.z", "", Some(b"1")),
            (b"a...z: 1", "a.b.z", "", None),

            // Matching among multiple entries
            (b"First: 1\nSecond: 2\n", "First", "", Some(b"1")),
            (b"First: 1\nSecond: 2\n", "Second", "", Some(b"2")),

            // Greediness
            (b"a*c.e: 1", "a.b.c.d.c.e", "", Some(b"1")),
            (b"a*c.e: 1", "a.b.c.c.e", "", Some(b"1")),
            (b"a*?.e: 1", "a.b.c.e", "", Some(b"1")),
            (b"a*c*e: 1", "a.b.c.d.c.d.e.d.e", "", Some(b"1")),

            // Precedence rules
            // Rule 1
            (b"First.second.third: 1\nFirst*third: 2\n", "First.second.third", "", Some(b"1")),
            (b"First*third: 2\nFirst.second.third: 1\n", "First.second.third", "", Some(b"1")),
            (b"First.second.third: 1\nFirst*third: 2\n", "x.x.x", "First.second.third", Some(b"1")),
            (b"First*third: 2\nFirst.second.third: 1\n", "x.x.x", "First.second.third", Some(b"1")),

            // Rule 2
            (b"First.second: 1\nFirst.third: 2\n", "First.second", "First.third", Some(b"1")),
            (b"First.third: 2\nFirst.second: 1\n", "First.second", "First.third", Some(b"1")),
            (b"First.second.third: 1\nFirst.?.third: 2\n", "First.second.third", "", Some(b"1")),
            (b"First.?.third: 2\nFirst.second.third: 1\n", "First.second.third", "", Some(b"1")),
            (b"First.second.third: 1\nFirst.?.third: 2\n", "x.x.x", "First.second.third", Some(b"1")),
            (b"First.?.third: 2\nFirst.second.third: 1\n", "x.x.x", "First.second.third", Some(b"1")),

            // Rule 3
            (b"First.second: 1\nFirst*second: 2\n", "First.second", "", Some(b"1")),
            (b"First*second: 2\nFirst.second: 1\n", "First.second", "", Some(b"1")),

            // Some real world examples. May contain duplicates to the above tests.

            // From the specification:
            // https://tronche.com/gui/x/xlib/resource-manager/matching-rules.html
            (b"xmh*Paned*activeForeground: red\n\
              *incorporate.Foreground: blue\n\
              xmh.toc*Command*activeForeground: green\n\
              xmh.toc*?.Foreground: white\n\
              xmh.toc*Command.activeForeground: black",
              "xmh.toc.messagefunctions.incorporate.activeForeground", "Xmh.Paned.Box.Command.Foreground", Some(b"black")),
            (b"urxvt*background: [95]#000", "urxvt.background", "", Some(b"[95]#000")),
            (b"urxvt*scrollBar_right:true", "urxvt.scrollBar_right", "", Some(b"true")),
            (b"urxvt*cutchars:    '\"'()*<>[]{|}", "urxvt.cutchars", "", Some(b"'\"'()*<>[]{|}")),
            (b"urxvt.keysym.Control-Shift-Up: perl:font:increment", "urxvt.keysym.Control-Shift-Up", "", Some(b"perl:font:increment")),
            (b"rofi.normal: #000000, #000000, #000000, #000000", "rofi.normal", "", Some(b"#000000, #000000, #000000, #000000")),
        ];
        let mut failures = 0;
        for &(data, resource, class, expected) in &tests {
            let mut entries = Vec::new();
            parse_database(data, &mut entries, |_, _| unreachable!());
            let result = match_entry(&entries, resource, class);
            if result != expected {
                eprintln!("While testing resource '{}' and class '{}' with the following input:", resource, class);
                eprintln!("{}", print_string(data));
                eprintln!("Expected: {:?}", expected.map(print_string));
                eprintln!("Got:      {:?}", result.map(print_string));
                eprintln!();
                failures += 1;
            }
        }
        if failures != 0 {
            panic!("Had {} failures", failures)
        }
    }

    fn print_string(data: &[u8]) -> String {
        std::str::from_utf8(data)
            .map(|s| s.to_string())
            .unwrap_or_else(|_| format!("{:?}", data))
    }
}
