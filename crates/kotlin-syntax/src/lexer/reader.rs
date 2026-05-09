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

    pub fn advance(&mut self) -> char {
        let c = self.peek();

        self.current += 1;

        c
    }

    pub fn is_at_end(&self) -> bool {
        self.current >= self.source.len()
    }

    pub fn start(&mut self) {
        self.start = self.current;
    }
}
