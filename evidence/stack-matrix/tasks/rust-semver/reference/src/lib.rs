//! Semantic versions and npm-style ranges. See README.md.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn error<T>(message: impl Into<String>) -> Result<T, Error> {
    Err(Error(message.into()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identifier {
    Numeric(u64),
    Alpha(String),
}

impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Identifier::Numeric(a), Identifier::Numeric(b)) => a.cmp(b),
            (Identifier::Numeric(_), Identifier::Alpha(_)) => Ordering::Less,
            (Identifier::Alpha(_), Identifier::Numeric(_)) => Ordering::Greater,
            (Identifier::Alpha(a), Identifier::Alpha(b)) => a.as_bytes().cmp(b.as_bytes()),
        }
    }
}

impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Identifier::Numeric(n) => write!(f, "{n}"),
            Identifier::Alpha(s) => f.write_str(s),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Vec<Identifier>,
    pub build: Vec<String>,
}

impl Version {
    fn new(major: u64, minor: u64, patch: u64) -> Self {
        Version { major, minor, patch, pre: Vec::new(), build: Vec::new() }
    }

    fn triple(&self) -> (u64, u64, u64) {
        (self.major, self.minor, self.patch)
    }
}

fn number(text: &str) -> Result<u64, Error> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return error(format!("`{text}` is not a number"));
    }
    if text.len() > 1 && text.starts_with('0') {
        return error(format!("`{text}` has a leading zero"));
    }
    text.parse().or_else(|_| error(format!("`{text}` is too large")))
}

fn valid_identifier(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

impl FromStr for Version {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let (rest, build) = match text.split_once('+') {
            Some((rest, build)) => {
                let parts: Vec<String> = build.split('.').map(str::to_owned).collect();
                if !parts.iter().all(|p| valid_identifier(p)) {
                    return error(format!("bad build metadata in `{text}`"));
                }
                (rest, parts)
            }
            None => (text, Vec::new()),
        };
        let (core, pre) = match rest.split_once('-') {
            Some((core, pre)) => {
                let mut identifiers = Vec::new();
                for part in pre.split('.') {
                    if !valid_identifier(part) {
                        return error(format!("bad prerelease in `{text}`"));
                    }
                    if part.bytes().all(|b| b.is_ascii_digit()) {
                        identifiers.push(Identifier::Numeric(number(part)?));
                    } else {
                        identifiers.push(Identifier::Alpha(part.to_owned()));
                    }
                }
                (core, identifiers)
            }
            None => (rest, Vec::new()),
        };
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3 {
            return error(format!("`{text}` is not MAJOR.MINOR.PATCH"));
        }
        Ok(Version {
            major: number(parts[0])?,
            minor: number(parts[1])?,
            patch: number(parts[2])?,
            pre,
            build,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            let pre: Vec<String> = self.pre.iter().map(ToString::to_string).collect();
            write!(f, "-{}", pre.join("."))?;
        }
        if !self.build.is_empty() {
            write!(f, "+{}", self.build.join("."))?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.triple().cmp(&other.triple()).then_with(|| {
            match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
}

#[derive(Debug, Clone)]
struct Comparator {
    op: Op,
    version: Version,
}

impl Comparator {
    fn holds(&self, v: &Version) -> bool {
        let order = v.cmp(&self.version);
        match self.op {
            Op::Eq => order == Ordering::Equal,
            Op::Gt => order == Ordering::Greater,
            Op::Ge => order != Ordering::Less,
            Op::Lt => order == Ordering::Less,
            Op::Le => order != Ordering::Greater,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Range {
    sets: Vec<Vec<Comparator>>,
}

const OPERATORS: [&str; 7] = [">=", "<=", ">", "<", "=", "^", "~"];

fn wildcard(part: &str) -> bool {
    matches!(part, "x" | "X" | "*")
}

fn between(low: Version, high: Version) -> Vec<Comparator> {
    vec![Comparator { op: Op::Ge, version: low }, Comparator { op: Op::Lt, version: high }]
}

fn comparators(token: &str) -> Result<Vec<Comparator>, Error> {
    let op = OPERATORS.iter().find(|op| token.starts_with(**op)).copied();
    let rest = &token[op.map_or(0, str::len)..];
    if op.is_some() && rest.is_empty() {
        return error(format!("`{token}` has no version"));
    }
    match op {
        Some("^") => {
            let v: Version = rest.parse()?;
            let high = if v.major > 0 {
                Version::new(v.major + 1, 0, 0)
            } else if v.minor > 0 {
                Version::new(0, v.minor + 1, 0)
            } else {
                Version::new(0, 0, v.patch + 1)
            };
            Ok(between(v, high))
        }
        Some("~") => {
            let parts: Vec<&str> = rest.split('.').collect();
            match parts.len() {
                3 => {
                    let v: Version = rest.parse()?;
                    let high = Version::new(v.major, v.minor + 1, 0);
                    Ok(between(v, high))
                }
                2 => {
                    let (major, minor) = (number(parts[0])?, number(parts[1])?);
                    Ok(between(Version::new(major, minor, 0), Version::new(major, minor + 1, 0)))
                }
                1 => {
                    let major = number(parts[0])?;
                    Ok(between(Version::new(major, 0, 0), Version::new(major + 1, 0, 0)))
                }
                _ => error(format!("`{token}` is not a version")),
            }
        }
        Some(op) => {
            let version: Version = rest.parse()?;
            let op = match op {
                ">=" => Op::Ge,
                "<=" => Op::Le,
                ">" => Op::Gt,
                "<" => Op::Lt,
                _ => Op::Eq,
            };
            Ok(vec![Comparator { op, version }])
        }
        None => {
            let parts: Vec<&str> = token.split('.').collect();
            if parts.len() == 3 && !parts.iter().any(|p| wildcard(p)) {
                return Ok(vec![Comparator { op: Op::Eq, version: token.parse()? }]);
            }
            if parts.len() > 3 {
                return error(format!("`{token}` is not a version"));
            }
            let fixed: Vec<&str> = parts.iter().copied().take_while(|p| !wildcard(p)).collect();
            if parts[fixed.len()..].iter().any(|p| !wildcard(p)) {
                return error(format!("`{token}` has a number after a wildcard"));
            }
            match fixed.len() {
                0 => Ok(Vec::new()),
                1 => {
                    let major = number(fixed[0])?;
                    Ok(between(Version::new(major, 0, 0), Version::new(major + 1, 0, 0)))
                }
                2 => {
                    let (major, minor) = (number(fixed[0])?, number(fixed[1])?);
                    Ok(between(Version::new(major, minor, 0), Version::new(major, minor + 1, 0)))
                }
                _ => error(format!("`{token}` is not a version")),
            }
        }
    }
}

fn comparator_set(text: &str) -> Result<Vec<Comparator>, Error> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() == 3 && words[1] == "-" {
        let low: Version = words[0].parse()?;
        let high: Version = words[2].parse()?;
        return Ok(vec![Comparator { op: Op::Ge, version: low }, Comparator { op: Op::Le, version: high }]);
    }
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if OPERATORS.contains(&words[i]) {
            match words.get(i + 1) {
                Some(next) => tokens.push(format!("{}{}", words[i], next)),
                None => return error(format!("`{}` has no version", words[i])),
            }
            i += 2;
        } else {
            tokens.push(words[i].to_owned());
            i += 1;
        }
    }
    let mut set = Vec::new();
    for token in tokens {
        set.extend(comparators(&token)?);
    }
    Ok(set)
}

impl FromStr for Range {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        if text.trim().is_empty() {
            return Ok(Range { sets: vec![Vec::new()] });
        }
        let mut sets = Vec::new();
        for part in text.split("||") {
            if part.trim().is_empty() {
                return error("an empty alternative");
            }
            sets.push(comparator_set(part)?);
        }
        Ok(Range { sets })
    }
}

impl Range {
    pub fn matches(&self, v: &Version) -> bool {
        self.sets.iter().any(|set| {
            set.iter().all(|c| c.holds(v))
                && (v.pre.is_empty()
                    || set.iter().any(|c| !c.version.pre.is_empty() && c.version.triple() == v.triple()))
        })
    }
}

pub fn max_satisfying<'a>(versions: &'a [Version], range: &Range) -> Option<&'a Version> {
    versions.iter().filter(|v| range.matches(v)).max()
}
