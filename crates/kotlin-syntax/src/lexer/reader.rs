pub struct SourceReader<'a> {
    source: &'a str,
    current: usize,
    start: usize,
}

impl<'a> SourceReader<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            current: 0,
            start: 0,
        }
    }

    pub fn peek(&self) -> char {
        self.peek_n(0)
    }

    pub fn peek_next(&self) -> char {
        self.peek_n(1)
    }

    pub fn peek_n(&self, n: usize) -> char {
        self.source[self.current + n..]
            .chars()
            .next()
            .unwrap_or('\0')
    }

    /// Move the cursor and return the advanced character
    pub fn advance(&mut self) -> char {
        let c = self.peek();

        self.current += 1;

        c
    }

    pub fn is_at_end(&self) -> bool {
        self.current >= self.source.len()
    }

    /// Start a new token
    pub fn new_token(&mut self) {
        self.start = self.current;
    }

    /// Get the start position (byte offset) of the token
    pub fn start(&self) -> usize {
        self.start
    }

    /// Get the current cursor position (byte offset)
    pub fn current(&self) -> usize {
        self.current
    }

    /// Get the current token lexeme
    pub fn current_lexeme(&self) -> &'a str {
        &self.source[self.start..self.current]
    }
}
