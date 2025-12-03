use crate::{
    syntax::highlighter::Highlighter,
    text_object::{character, cursor::Cursor, line_break::LineBreak},
};

use std::collections::HashMap;
use std::ops::{Add, Sub};

use ropey::Rope;
use tree_sitter::Tree;

#[derive(Debug, Clone, PartialEq)]
pub struct Readonly;
#[derive(Debug, Clone, PartialEq)]
pub struct Write;

#[derive(Debug, Clone, PartialEq)]
pub struct TextObject<State = Readonly> {
    content: Rope,
    state: std::marker::PhantomData<State>,
    line_break: LineBreak,
}

impl<State> Default for TextObject<State> {
    fn default() -> Self {
        let content = String::default();

        TextObject {
            content: Rope::from_str(&content),
            state: std::marker::PhantomData,
            line_break: LineBreak::Lf,
        }
    }
}

impl TextObject<Readonly> {
    pub fn from(content: &str) -> TextObject<Readonly> {
        let content = Rope::from_str(content);
        let line_break = match content.line(0).to_string().contains("\r\n") {
            true => LineBreak::Crlf,
            false => LineBreak::Lf,
        };
        TextObject::<Readonly> {
            content,
            state: std::marker::PhantomData::<Readonly>,
            line_break,
        }
    }

    pub fn with_write(self) -> TextObject<Write> {
        TextObject::<Write> {
            content: self.content,
            state: std::marker::PhantomData,
            line_break: self.line_break,
        }
    }
}

impl TextObject<Write> {
    pub fn insert_char(&mut self, c: char, cursor: &Cursor) {
        let line = self.content.line_to_char(cursor.row());
        let col_offset = line + cursor.col();
        self.content.insert_char(col_offset, c);
    }

    pub fn insert_newline(&mut self, cursor: &Cursor) {
        let line = self.content.line_to_char(cursor.row());
        let col_offset = line + cursor.col();
        self.content
            .insert(col_offset, &self.line_break.to_string());
    }

    pub fn erase_backwards_up_to_line_start(&mut self, cursor: &Cursor) {
        if cursor.col().eq(&0) {
            return;
        }
        let line = self.content.line_to_char(cursor.row());
        let col_offset = line + cursor.col();
        self.content
            .try_remove(col_offset.saturating_sub(1)..col_offset)
            .ok();
    }

    pub fn erase_previous_char(&mut self, cursor: &Cursor) {
        let line = self.content.line_to_char(cursor.row());
        let col_offset = line + cursor.col();
        self.content
            .try_remove(col_offset.saturating_sub(1)..col_offset)
            .ok();
    }

    pub fn erase_current_char(&mut self, cursor: &Cursor) {
        let line = self.content.line_to_char(cursor.row());
        let col_offset = line + cursor.col();
        self.content.try_remove(col_offset..col_offset.add(1)).ok();
    }

    pub fn current_line(&self, cursor: &Cursor) -> Option<&str> {
        self.content.line(cursor.row()).as_str()
    }

    pub fn line_len_with_linebreak(&self, line: usize) -> usize {
        self.content
            .line(line)
            .as_str()
            .map(|line| line.len())
            .unwrap_or_default()
    }

    pub fn line_len(&self, line: usize) -> usize {
        self.content
            .line(line)
            .as_str()
            .map(|line| line.len().saturating_sub(self.line_break.clone().into()))
            .unwrap_or_default()
    }

    pub fn erase_until_eol(&mut self, cursor: &Cursor) {
        let line = self.content.line_to_char(cursor.row());
        let next_line = self.content.line_to_char(cursor.row().add(1));
        let col_offset = line + cursor.col();
        self.content
            .try_remove(col_offset..next_line.saturating_sub(1))
            .ok();
    }

    pub fn find_char_before_whitespace(&self, cursor: &Cursor) -> (usize, usize) {
        let line = self.content.line_to_char(cursor.row());
        let col_offset = line + cursor.col();
        let mut found = false;
        let mut index = col_offset.saturating_sub(1);

        // TODO refactor to use character module
        for _ in (0..col_offset.saturating_sub(1)).rev() {
            let char = self.content.char(index);
            match (char, found) {
                (c, false) if c.is_whitespace() => found = true,
                (c, true) if !c.is_whitespace() => break,
                _ => {}
            }
            index = index.saturating_sub(1);
        }

        let curr_row = self.content.char_to_line(index);
        let curr_row_start = self.content.line_to_char(curr_row);
        let curr_col = index - curr_row_start;

        (curr_col, curr_row)
    }

    pub fn find_next_word(&self, cursor: &Cursor, bigword: &bool) -> (usize, usize) {
        let count = 1; // TODO pass as arg

        let start_idx = self.to_offset_cursor(cursor);
        let mut end_idx = start_idx;
        let mut found_newline = false;

        for _ in 0..count {
            if end_idx > self.content.len_chars() {
                break;
            }

            // move to end of current word
            if !self.is_whitespace(self.get_char(end_idx)) {
                let initial_char_kind = self.get_char_kind(self.get_char(end_idx), bigword);

                while end_idx < self.content.len_chars()
                    && self.get_char_kind(self.get_char(end_idx), bigword) == initial_char_kind
                {
                    end_idx = end_idx.saturating_add(1);
                }
            }

            while end_idx < self.content.len_chars() && self.is_whitespace(self.get_char(end_idx)) {
                match self.get_char(end_idx) {
                    Some('\n') => {
                        // return early if a second newline is found
                        if found_newline {
                            return self.col_row_from_offset(end_idx);
                        } else {
                            found_newline = true;
                            end_idx = end_idx.saturating_add(1);
                        }
                    }
                    _ => end_idx = end_idx.saturating_add(1),
                }
            }
        }

        self.col_row_from_offset(end_idx)
    }

    pub fn find_prev_word(&self, cursor: &Cursor) -> (usize, usize) {
        let bigword = false; // TODO pass this in as arg
        let count = 1; // TODO pass this in as arg

        let start_idx = self.to_offset_cursor(cursor);
        let mut end_idx = start_idx;
        let mut found_newline = false;

        for _ in 0..count {
            // skip trailing whitespace
            while end_idx > 0 && self.is_whitespace(self.get_char(end_idx - 1)) {
                match self.get_char(end_idx - 1) {
                    Some('\n') => {
                        // stop at the second newline found
                        if found_newline {
                            // return here since we're two loops deep
                            return self.col_row_from_offset(end_idx);
                        } else {
                            found_newline = true;
                            end_idx = end_idx.saturating_sub(1);
                        }
                    }
                    _ => end_idx = end_idx.saturating_sub(1),
                };
            }

            if end_idx == 0 {
                break;
            }

            let initial_char_type = self.get_char_kind(self.get_char(end_idx - 1), &bigword);
            while end_idx > 0
                && self.get_char_kind(self.get_char(end_idx - 1), &bigword) == initial_char_type
            {
                end_idx = end_idx.saturating_sub(1);
            }
        }

        self.col_row_from_offset(end_idx)
    }

    pub fn find_word_end(&self, cursor: &Cursor, bigword: &bool) -> (usize, usize) {
        // starting at the next character so we don't get stuck on single length string
        let start_idx = self.to_offset_cursor(cursor) + 1;
        let mut end_idx = self.skip_whitespace_forward(start_idx, bigword);

        // can assume we're in word now, find the end
        if let Some(initial_char) = self.content.get_char(end_idx) {
            for char in self.content.chars_at(end_idx + 1) {
                if character::kind(char, bigword) != character::kind(initial_char, bigword) {
                    break;
                }
                end_idx = end_idx.add(1);
            }
        }

        self.col_row_from_offset(end_idx)
    }

    pub fn find_empty_line_above(&self, cursor: &Cursor) -> usize {
        let mut new_row = cursor.row().saturating_sub(1);

        while let Some(line) = self.content.get_line(new_row) {
            if line.to_string().eq(&self.line_break.to_string()) {
                break;
            }

            if new_row.eq(&0) {
                break;
            }
            new_row = new_row.saturating_sub(1);
        }

        new_row
    }

    pub fn find_empty_line_below(&self, cursor: &Cursor) -> usize {
        let mut new_row = cursor.row().add(1);
        let len_lines = self.len_lines();

        while let Some(line) = self.content.get_line(new_row) {
            if line.to_string().eq(&self.line_break.to_string()) {
                break;
            }
            new_row = new_row.add(1);
        }

        usize::min(new_row, len_lines.saturating_sub(1))
    }

    pub fn len_lines(&self) -> usize {
        self.content.len_lines()
    }

    pub fn delete_line(&mut self, line: usize) {
        let start = self.content.line_to_char(line);
        let end = self.content.line_to_char(line.add(1));
        self.content.try_remove(start..end).ok();
    }

    /// deletes a word forward in one of two ways:
    ///
    /// - if the current character is alphanumeric, then this delete up to the first non alphanumeric character
    /// - if the current character is non alphanumeric, then delete up to the first alphanumeric character
    pub fn delete_word(&mut self, cursor: &Cursor) {
        let start_idx = self.content.line_to_char(cursor.row()).add(cursor.col());
        let mut end_idx = start_idx.saturating_sub(1);

        if let Some(initial_char) = self.content.get_char(start_idx) {
            for char in self.content.chars_at(start_idx) {
                match (initial_char.is_alphanumeric(), char.is_alphanumeric()) {
                    (false, _) if self.line_break.to_string().contains(char) => break,
                    (false, true) => {
                        end_idx = end_idx.add(1);
                        break;
                    }
                    (true, false) => {
                        end_idx = end_idx.add(1);
                        break;
                    }
                    _ => end_idx = end_idx.add(1),
                }
            }

            self.content.try_remove(start_idx..end_idx).ok();
        }
    }

    /// deletes a word backwards in one of two ways:
    ///
    /// - if the current character is alphanumeric, then this delete up to the first non alphanumeric character
    /// - if the current character is non alphanumeric, then delete up to the first alphanumeric character
    ///
    /// will always return how many columns to advance the cursor
    pub fn delete_word_backwards(&mut self, cursor: &Cursor) -> usize {
        let start_idx = self.content.line_to_char(cursor.row()).add(cursor.col());
        let mut end_idx = start_idx.saturating_sub(1);

        if let Some(initial_char) = self.content.get_char(start_idx.saturating_sub(1)) {
            for _ in (0..start_idx.saturating_sub(1)).rev() {
                let char = self.content.char(end_idx);
                match (initial_char.is_alphanumeric(), char.is_alphanumeric()) {
                    (false, _) if self.line_break.to_string().contains(char) => break,
                    (false, true) => break,
                    (true, false) => break,
                    _ => end_idx = end_idx.saturating_sub(1),
                }
            }
        };

        self.content.try_remove(end_idx.add(1)..start_idx).ok();
        start_idx.sub(end_idx.add(1))
    }

    pub fn insert_line_below(&mut self, cursor: &Cursor, tree: Option<&Tree>) {
        let indentation = self.get_scope_aware_indentation(cursor, tree);
        let next_line = self.content.line_to_char(cursor.row().add(1));
        let line_with_indentation = format!("{}{}", indentation, &self.line_break.to_string());
        self.content.insert(next_line, &line_with_indentation);
    }

    pub fn insert_line_above(&mut self, cursor: &Cursor, tree: Option<&Tree>) {
        let indentation = self.get_scope_aware_indentation(cursor, tree);
        let curr_line = self.content.line_to_char(cursor.row());
        let line_with_indentation = format!("{}{}", indentation, &self.line_break.to_string());
        self.content.insert(curr_line, &line_with_indentation);
    }

    pub fn find_oposing_token(&mut self, cursor: &Cursor) -> (usize, usize) {
        let start_idx = self.content.line_to_char(cursor.row()).add(cursor.col());
        let mut combinations = HashMap::new();
        let pairs = [('<', '>'), ('(', ')'), ('[', ']'), ('{', '}')];
        pairs.iter().for_each(|pair| {
            combinations.insert(pair.0, pair.1);
            combinations.insert(pair.1, pair.0);
        });

        let mut look_forward = true;
        let mut token_to_search = char::default();
        let (mut curr_open, mut walked) = (0, 0);

        if let Some(initial_char) = self.content.get_char(start_idx) {
            match initial_char {
                c if character::is_opening_token(c) => {
                    token_to_search = *combinations.get(&c).unwrap();
                    curr_open = curr_open.add(1);
                }
                c if character::is_closing_token(c) => {
                    token_to_search = *combinations.get(&c).unwrap();
                    curr_open = curr_open.add(1);
                    look_forward = false;
                }
                _ => {}
            }

            let range = if look_forward {
                start_idx.add(1)..self.content.len_chars()
            } else {
                0..start_idx
            };

            for i in range {
                let char = self
                    .content
                    .get_char(if look_forward {
                        i
                    } else {
                        start_idx - walked - 1
                    })
                    .unwrap_or_default();

                if token_to_search.eq(&char::default()) {
                    if !character::is_opening_token(char) {
                        walked = walked.add(1);
                        continue;
                    }
                    token_to_search = *combinations.get(&char).unwrap();
                }

                char.eq(combinations.get(&token_to_search).unwrap())
                    .then(|| curr_open = curr_open.add(1));

                char.eq(&token_to_search)
                    .then(|| curr_open = curr_open.sub(1));

                walked = walked.add(1);

                if curr_open.eq(&0) {
                    break;
                }
            }
        }
        if curr_open.gt(&0) {
            return (cursor.col(), cursor.row());
        }

        if look_forward {
            let curr_row = self.content.char_to_line(start_idx.add(walked));
            let curr_row_start = self.content.line_to_char(curr_row);
            let curr_col = start_idx.add(walked).saturating_sub(curr_row_start);
            (curr_col, curr_row)
        } else {
            let curr_row = self.content.char_to_line(start_idx.sub(walked));
            let curr_row_start = self.content.line_to_char(curr_row);
            let curr_col = start_idx.sub(walked).sub(curr_row_start);
            (curr_col, curr_row)
        }
    }

    fn to_offset_cursor(&self, cursor: &Cursor) -> usize {
        self.to_offset(cursor.col(), cursor.row())
    }

    fn to_offset(&self, col: usize, row: usize) -> usize {
        self.content.line_to_char(row).add(col)
    }

    fn col_row_from_offset(&self, idx: usize) -> (usize, usize) {
        let row = self.content.char_to_line(idx);
        let row_start = self.content.line_to_char(row);
        let col = idx.sub(row_start);

        (col, row)
    }

    fn get_scope_aware_indentation(&self, cursor: &Cursor, tree: Option<&Tree>) -> String {
        if let Some(tree) = tree {
            let line_byte_idx = self.content.line_to_byte(cursor.row());
            let cursor_byte_idx = line_byte_idx.add(cursor.col());
            let indentation_level = Highlighter::find_indentation_level(tree, cursor_byte_idx);
            "  ".repeat(indentation_level)
        } else {
            String::new()
        }
    }

    fn skip_whitespace_forward(&self, start_idx: usize, bigword: &bool) -> usize {
        let mut end_idx = start_idx;
        // skip past initial whitespace to first char of a word or punctuation
        if let Some(initial_char) = self.content.get_char(start_idx) {
            if character::kind(initial_char, bigword) == character::Kind::Whitespace {
                for char in self.content.chars_at(start_idx + 1) {
                    end_idx = end_idx.add(1);
                    if character::kind(char, bigword) != character::Kind::Whitespace {
                        break;
                    }
                }
            }
        }

        end_idx
    }

    fn is_whitespace(&self, c: Option<char>) -> bool {
        match c {
            Some(c) => character::kind(c, &false) == character::Kind::Whitespace,
            None => false,
        }
    }

    fn get_char(&self, idx: usize) -> Option<char> {
        self.content.get_char(idx)
    }

    fn get_char_kind(&self, c: Option<char>, bigword: &bool) -> character::Kind {
        match c {
            Some(c) => character::kind(c, bigword),
            None => character::Kind::Unknown,
        }
    }
}

impl<State> std::fmt::Display for TextObject<State> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.content.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(content: &str) -> (TextObject<Write>, Cursor) {
        (TextObject::from(content).with_write(), Cursor::default())
    }

    fn create_long_word() -> String {
        let mut content = String::new();
        for _ in 0..1000 {
            content = content.add("a");
        }
        content
    }

    #[test]
    pub fn insert_char() {
        let (mut object, cur) = setup("");
        object.insert_char('a', &cur);
        assert_eq!("a", object.content.to_string())
    }

    mod find_word_end {
        use super::*;

        #[test]
        pub fn simple_word() {
            let (object, cur) = setup("hello");
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('o', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(4, col);
        }

        #[test]
        pub fn from_middle() {
            let (object, mut cur) = setup("hello");
            cur.move_right(2);
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('o', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(4, col);
        }

        #[test]
        pub fn multiple_words() {
            let (object, cur) = setup("foo bar baz");
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('o', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(2, col);
        }

        #[test]
        pub fn skip_leading_whitespace() {
            let (object, cur) = setup(" \tword");
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('d', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn skip_multiple_spaces() {
            let (object, mut cur) = setup("foo    bar");
            cur.move_right(2);
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('r', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(9, col);
        }

        #[test]
        pub fn stops_at_punctuation() {
            let (object, cur) = setup("hello,world");
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('o', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(4, col);
        }

        #[test]
        pub fn punctuation_as_word() {
            let (object, cur) = setup("!!!");
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('!', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(2, col);
        }

        #[test]
        pub fn mixed_alphanumeric() {
            let (object, cur) = setup("test123");
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('3', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(6, col);
        }

        #[test]
        pub fn single_character() {
            let (object, cur) = setup("a b");
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('b', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(2, col);
        }

        #[test]
        pub fn end_of_line() {
            let (object, mut cur) = setup("word");
            cur.move_right(3);
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!(Option::None, object.get_char(object.to_offset(col, row)));
            assert_eq!(0, row);
            assert_eq!(4, col);
        }

        #[test]
        pub fn empty_line() {
            let (object, cur) = setup("\n\nword");
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('d', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(2, row);
            assert_eq!(3, col);
        }

        #[test]
        pub fn underscore_word() {
            let (object, cur) = setup("test_case");
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('e', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(8, col);
        }

        #[test]
        pub fn unicode_characters() {
            let (object, cur) = setup("résumé");
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('é', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn multibyte_sequences() {
            let (object, cur) = setup("世界");
            let (col, row) = object.find_word_end(&cur, &false);
            assert_eq!('界', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(1, col);
        }

        #[test]
        pub fn bigword_all_punctuation_and_special_chars() {
            let (object, cur) = setup("t.,<>?/{}[]\\|=+-_!@#$%^&*();:'\"`~");
            let (col, row) = object.find_word_end(&cur, &true);
            assert_eq!('~', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(32, col);
        }
    }

    mod find_next_word {
        use super::*;

        #[test]
        pub fn from_middle_of_word_to_next() {
            let (object, mut cur) = setup("test phrase");
            cur.move_right(2);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('p', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn from_end_of_word_to_next() {
            let (object, mut cur) = setup("test phrase");
            cur.move_right(3);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('p', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn between_space_separated_words() {
            let (object, mut cur) = setup("test phrase");
            cur.move_right(4);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('p', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn within_keyword_characters() {
            let (object, mut cur) = setup("foo_bar");
            cur.move_right(2);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!(Option::None, object.get_char(object.to_offset(col, row)));
            assert_eq!(0, row);
            assert_eq!(7, col);
        }

        #[test]
        pub fn keyword_to_punctuation() {
            let (object, mut cur) = setup("foo,bar");
            cur.move_right(2);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!(',', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(3, col);
        }

        #[test]
        pub fn punctuation_to_keyword() {
            let (object, mut cur) = setup("foo,bar");
            cur.move_right(3);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('b', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(4, col);
        }

        #[test]
        pub fn consecutive_punctuation() {
            let (object, mut cur) = setup("foo!!");
            cur.move_right(3);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!(Option::None, object.get_char(object.to_offset(col, row)));
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn multiple_spaces() {
            let (object, cur) = setup("one  two");
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('t', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn mixed_spaces_and_tabs() {
            let (object, mut cur) = setup("one \ttwo");
            cur.move_right(3);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('t', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn to_next_line() {
            let (object, mut cur) = setup("word\nnext");
            cur.move_right(3);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('n', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(1, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn across_empty_line() {
            let (object, mut cur) = setup("word\n\nnext");
            cur.move_right(3);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('\n', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(1, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn across_multiple_empty_lines() {
            let (object, mut cur) = setup("word\n\n\nnext");
            cur.move_right(3);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('\n', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(1, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn at_file_end_no_op() {
            let (object, mut cur) = setup("foo");
            cur.move_right(3);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!(Option::None, object.get_char(object.to_offset(col, row)));
            assert_eq!(0, row);
            assert_eq!(3, col);
        }

        #[test]
        pub fn empty_file() {
            let (object, cur) = setup("");
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!(0, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn whitespace_only_line() {
            let (object, mut cur) = setup("word  \nnext");
            cur.move_right(3);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('n', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(1, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn very_long_word() {
            let content = create_long_word();
            let (object, cur) = setup(&content);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!(0, row);
            assert_eq!(content.len(), col);
        }

        #[test]
        pub fn punctuation_only_word() {
            let (object, cur) = setup("word !!!");
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('!', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn special_char_to_keyword() {
            let (object, cur) = setup("$foo");
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('f', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(1, col);
        }

        #[test]
        pub fn unicode_characters() {
            let (object, cur) = setup("café résumé");
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('r', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn multibyte_sequences() {
            let (object, cur) = setup("世界 hello");
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('h', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(3, col);
        }

        #[test]
        pub fn keyword_punctuation_keyword() {
            let (object, mut cur) = setup("foo()bar");
            cur.move_right(3);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('b', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn keyword_punctuation_at_line_end() {
            let (object, mut cur) = setup("word,\nnext");
            cur.move_right(4);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('n', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(1, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn from_whitespace_between_words() {
            let (object, mut cur) = setup("word   next");
            cur.move_right(5);
            let (col, row) = object.find_next_word(&cur, &false);
            assert_eq!('n', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(7, col);
        }

        #[test]
        pub fn bigword_all_punctuation_and_special_chars() {
            let (object, cur) = setup("t.,<>?/{}[]\\|=+-_!@#$%^&*();:'\"`~ newword");

            let (col, row) = object.find_next_word(&cur, &true);

            assert_eq!('n', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(34, col);
        }
    }

    mod find_prev_word {
        use super::*;

        #[test]
        pub fn from_middle_of_word_moves_to_word_start() {
            let (object, mut cur) = setup("myphrase");
            cur.move_right(3);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('m', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn from_word_start_moves_to_previous_word_start_same_line() {
            let (object, mut cur) = setup("first second");
            cur.move_right(6);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('f', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn from_whitespace_between_words_moves_to_prev_word_start() {
            let (object, mut cur) = setup("foo bar  baz");
            cur.move_right(7);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('b', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(4, col);
        }

        #[test]
        pub fn from_trailing_whitespace_at_line_end_moves_to_last_word_start() {
            let (object, mut cur) = setup("bar\t ");
            cur.move_right(4);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('b', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn from_line_start_moves_to_last_word_previous_line() {
            let (object, mut cur) = setup("foo\nbar");
            cur.move_right(4);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('f', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn stops_at_buffer_start_when_no_previous_word() {
            let (object, cur) = setup("foo");
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('f', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn treats_punctuation_as_separate_word_segment() {
            let (object, mut cur) = setup("foo.bar");
            cur.move_right(4);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('.', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(3, col);
        }

        #[test]
        pub fn from_inside_punctuation_runs_to_punctuation_start() {
            let (object, mut cur) = setup("foo !!!");
            cur.move_right(5);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('!', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(4, col);
        }

        #[test]
        pub fn with_empty_line_stops_at_newline() {
            let (object, mut cur) = setup("foo\n\nbar");
            cur.move_right(5);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('\n', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(1, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn over_tabs_and_spaces_to_previous_word() {
            let (object, mut cur) = setup("foo\t bar");
            cur.move_right(5);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('f', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn from_nonletter_word_moves_to_previous_word() {
            let (object, mut cur) = setup("123");
            cur.move_right(1);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('1', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn from_before_first_nonblank_moves_to_previous_line_nonblank() {
            let (object, mut cur) = setup("foo\n\t bar");
            cur.move_right(6);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('f', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn empty_file() {
            let (object, cur) = setup("");
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!(0, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn very_long_word() {
            let content = create_long_word();
            let (object, mut cur) = setup(&content);
            cur.move_right(content.len() - 1);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!(0, row);
            assert_eq!(0, col);
        }

        #[test]
        pub fn unicode_characters() {
            let (object, mut cur) = setup("café résumé");
            cur.move_right(10);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('r', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(5, col);
        }

        #[test]
        pub fn multibyte_sequences() {
            let (object, mut cur) = setup("hello 世界|");
            cur.move_right(8);
            let (col, row) = object.find_prev_word(&cur);

            assert_eq!('世', object.get_char(object.to_offset(col, row)).unwrap());
            assert_eq!(0, row);
            assert_eq!(6, col);
        }
    }
}
