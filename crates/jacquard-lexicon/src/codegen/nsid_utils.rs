//! Utilities for parsing and working with NSIDs and refs

/// Parsed NSID components for easier manipulation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NsidPath<'a> {
    nsid: &'a str,
    segments: Vec<&'a str>,
}

impl<'a> NsidPath<'a> {
    /// Parse an NSID into its component segments
    pub fn parse(nsid: &'a str) -> Self {
        let segments: Vec<&str> = nsid.split('.').collect();
        Self { nsid, segments }
    }

    /// Get the namespace (first two segments joined with '.')
    /// Returns "com.atproto" from "com.atproto.repo.strongRef"
    pub fn namespace(&self) -> String {
        if self.segments.len() >= 2 {
            format!("{}.{}", self.segments[0], self.segments[1])
        } else {
            self.nsid.to_string()
        }
    }

    /// Get the last segment of the NSID
    pub fn last_segment(&self) -> &str {
        self.segments.last().copied().unwrap_or(self.nsid)
    }

    /// Get all segments except the last
    pub fn parent_segments(&self) -> &[&str] {
        if self.segments.is_empty() {
            &[]
        } else {
            &self.segments[..self.segments.len() - 1]
        }
    }

    /// Check if this is a "defs" NSID (ends with "defs")
    pub fn is_defs(&self) -> bool {
        self.last_segment() == "defs"
    }

    /// Get all segments
    pub fn segments(&self) -> &[&str] {
        &self.segments
    }

    /// Get the original NSID string
    pub fn as_str(&self) -> &str {
        self.nsid
    }

    /// Get number of segments
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Check if empty (should not happen with valid NSIDs)
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
}

/// Parsed reference with NSID and optional fragment
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefPath<'a> {
    nsid: &'a str,
    def: &'a str,
}

impl<'a> RefPath<'a> {
    /// Parse a reference string, normalizing it based on current NSID context
    pub fn parse(ref_str: &'a str, current_nsid: Option<&'a str>) -> Self {
        if let Some(fragment) = ref_str.strip_prefix('#') {
            // Local ref: #option → use current_nsid
            let nsid = current_nsid.unwrap_or("");
            Self {
                nsid,
                def: fragment,
            }
        } else if let Some((nsid, def)) = ref_str.split_once('#') {
            // Full ref with fragment: nsid#def
            Self { nsid, def }
        } else {
            // Full ref without fragment: nsid (implicit "main")
            Self {
                nsid: ref_str,
                def: "main",
            }
        }
    }

    /// Get the NSID portion of the ref
    pub fn nsid(&self) -> &str {
        self.nsid
    }

    /// Get the def name (fragment) portion of the ref
    pub fn def(&self) -> &str {
        self.def
    }

    /// Check if this is a local ref (was parsed from #fragment)
    pub fn is_local(&self, current_nsid: &str) -> bool {
        self.nsid == current_nsid && self.def != "main"
    }

    /// Get the full ref string (nsid#def)
    pub fn full_ref(&self) -> String {
        if self.def == "main" {
            self.nsid.to_string()
        } else {
            format!("{}#{}", self.nsid, self.def)
        }
    }

    /// Normalize a local ref by prepending the current NSID if needed
    /// Returns the normalized ref string suitable for corpus lookup
    pub fn normalize(ref_str: &str, current_nsid: &str) -> String {
        if ref_str.starts_with('#') {
            format!("{}{}", current_nsid, ref_str)
        } else {
            ref_str.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nsid_path_parse() {
        let path = NsidPath::parse("com.atproto.repo.strongRef");
        assert_eq!(path.segments(), &["com", "atproto", "repo", "strongRef"]);
        assert_eq!(path.namespace(), "com.atproto");
        assert_eq!(path.last_segment(), "strongRef");
        assert_eq!(path.parent_segments(), &["com", "atproto", "repo"]);
        assert!(!path.is_defs());
    }

    #[test]
    fn test_nsid_path_defs() {
        let path = NsidPath::parse("com.atproto.label.defs");
        assert!(path.is_defs());
        assert_eq!(path.last_segment(), "defs");
    }

    #[test]
    fn test_ref_path_local() {
        let ref_path = RefPath::parse("#option", Some("com.example.foo"));
        assert_eq!(ref_path.nsid(), "com.example.foo");
        assert_eq!(ref_path.def(), "option");
        assert!(ref_path.is_local("com.example.foo"));
        assert_eq!(ref_path.full_ref(), "com.example.foo#option");
    }

    #[test]
    fn test_ref_path_with_fragment() {
        let ref_path = RefPath::parse("com.example.foo#bar", None);
        assert_eq!(ref_path.nsid(), "com.example.foo");
        assert_eq!(ref_path.def(), "bar");
        assert!(!ref_path.is_local("com.other.baz"));
        assert_eq!(ref_path.full_ref(), "com.example.foo#bar");
    }

    #[test]
    fn test_ref_path_implicit_main() {
        let ref_path = RefPath::parse("com.example.foo", None);
        assert_eq!(ref_path.nsid(), "com.example.foo");
        assert_eq!(ref_path.def(), "main");
        assert_eq!(ref_path.full_ref(), "com.example.foo");
    }

    #[test]
    fn test_ref_path_normalize() {
        assert_eq!(
            RefPath::normalize("#option", "com.example.foo"),
            "com.example.foo#option"
        );
        assert_eq!(
            RefPath::normalize("com.other.bar#baz", "com.example.foo"),
            "com.other.bar#baz"
        );
    }
}
