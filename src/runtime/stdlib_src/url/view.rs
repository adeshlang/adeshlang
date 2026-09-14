//! URLView Borrowed Zero-Copy Parser for AdeshLang.

use super::url_object::URL;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct URLView<'a> {
    source: &'a str,
    scheme_end: usize,
    host_start: Option<usize>,
    host_end: Option<usize>,
    port_start: Option<usize>,
    port_end: Option<usize>,
    path_start: usize,
    path_end: usize,
    query_start: Option<usize>,
    query_end: Option<usize>,
    fragment_start: Option<usize>,
}

impl<'a> URLView<'a> {
    pub fn parse(input: &'a str) -> Result<Self, String> {
        let input = input.trim();
        let colon_pos = input
            .find(':')
            .ok_or_else(|| "Missing scheme in URLView".to_string())?;

        let scheme_end = colon_pos;
        let rest = &input[colon_pos + 1..];

        let (
            host_start,
            host_end,
            port_start,
            port_end,
            path_start,
            path_end,
            query_start,
            query_end,
            fragment_start,
        ) = if rest.starts_with("//") {
            let auth_and_rest = &rest[2..];
            let auth_start_idx = colon_pos + 1 + 2;

            let slash_pos = auth_and_rest.find('/').map(|p| auth_start_idx + p);
            let q_pos = auth_and_rest.find('?').map(|p| auth_start_idx + p);
            let frag_pos = auth_and_rest.find('#').map(|p| auth_start_idx + p);

            let path_st = slash_pos.or(q_pos).or(frag_pos).unwrap_or(input.len());

            let authority_str = &input[auth_start_idx..path_st];
            let host_st = auth_start_idx;
            let (h_end, p_st, p_end) = if let Some(p_colon) = authority_str.rfind(':') {
                (
                    host_st + p_colon,
                    Some(host_st + p_colon + 1),
                    Some(path_st),
                )
            } else {
                (path_st, None, None)
            };

            let path_ed = q_pos.or(frag_pos).unwrap_or(input.len());

            let (q_st, q_ed) = if let Some(q) = q_pos {
                let ed = frag_pos.unwrap_or(input.len());
                (Some(q + 1), Some(ed))
            } else {
                (None, None)
            };

            let f_st = frag_pos.map(|f| f + 1);

            (
                Some(host_st),
                Some(h_end),
                p_st,
                p_end,
                path_st,
                path_ed,
                q_st,
                q_ed,
                f_st,
            )
        } else {
            let path_st = colon_pos + 1;
            let q_pos = input.find('?');
            let frag_pos = input.find('#');

            let path_ed = q_pos.or(frag_pos).unwrap_or(input.len());
            let (q_st, q_ed) = if let Some(q) = q_pos {
                let ed = frag_pos.unwrap_or(input.len());
                (Some(q + 1), Some(ed))
            } else {
                (None, None)
            };

            let f_st = frag_pos.map(|f| f + 1);

            (None, None, None, None, path_st, path_ed, q_st, q_ed, f_st)
        };

        Ok(Self {
            source: input,
            scheme_end,
            host_start,
            host_end,
            port_start,
            port_end,
            path_start,
            path_end,
            query_start,
            query_end,
            fragment_start,
        })
    }

    pub fn scheme(&self) -> &'a str {
        &self.source[..self.scheme_end]
    }

    pub fn host(&self) -> Option<&'a str> {
        match (self.host_start, self.host_end) {
            (Some(st), Some(ed)) => Some(&self.source[st..ed]),
            _ => None,
        }
    }

    pub fn port_str(&self) -> Option<&'a str> {
        match (self.port_start, self.port_end) {
            (Some(st), Some(ed)) => Some(&self.source[st..ed]),
            _ => None,
        }
    }

    pub fn path(&self) -> &'a str {
        if self.path_start >= self.source.len() {
            "/"
        } else {
            &self.source[self.path_start..self.path_end]
        }
    }

    pub fn query(&self) -> Option<&'a str> {
        match (self.query_start, self.query_end) {
            (Some(st), Some(ed)) => Some(&self.source[st..ed]),
            _ => None,
        }
    }

    pub fn fragment(&self) -> Option<&'a str> {
        self.fragment_start.map(|st| &self.source[st..])
    }

    pub fn to_owned(&self) -> Result<URL, String> {
        URL::parse(self.source)
    }
}
