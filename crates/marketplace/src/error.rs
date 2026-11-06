//! Error types. Messages are fixed text or sanitized names: they never contain `git` stderr,
//! URLs with userinfo, or other attacker- or credential-bearing text (ADR-015).

use std::fmt;

/// Makes an untrusted string safe to show: control and invisible characters replaced, shortened
/// to 80 characters.
pub fn show(text: &str) -> String {
    let mut out: String = text
        .chars()
        .take(80)
        .map(|c| {
            if c.is_control() || crate::manifest::hidden_char(c) {
                '?'
            } else {
                c
            }
        })
        .collect();
    if text.chars().count() > 80 {
        out.push_str("...");
    }
    out
}

/// Why a Git URL was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlError {
    Empty,
    TooLong,
    /// Whitespace, control characters, non-ASCII or a character no URL here needs.
    ForbiddenCharacter,
    /// Not `https://`, `ssh://`, `git@host:path` or `owner/repo`.
    UnsupportedScheme,
    /// A password or token, or any userinfo where none is allowed.
    Credentials,
    /// Option-looking, malformed or oversized host.
    BadHost,
    /// A private host (IPv4 address, `localhost`, one-label name, local-network suffix) where
    /// none may be used: an activity source listed in an index.
    LocalHost,
    /// An ssh address listed as an activity's source. Sources must be https, because only an
    /// https fetch can be pinned to the addresses Lullshift checked (security review MEDIUM-1).
    SourceNotHttps,
    /// A private host, or a name that resolves to a private address, for a marketplace the
    /// user has not confirmed as being on their own computer or network.
    PrivateNotConfirmed,
    /// An address that can never be a Git server: unspecified, link-local (cloud metadata),
    /// multicast, broadcast or reserved.
    SpecialAddress,
    BadPort,
    BadPath,
    BadShorthand,
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "the repository address is empty",
            Self::TooLong => "the repository address is too long",
            Self::ForbiddenCharacter => "the repository address has a character that isn't allowed",
            Self::UnsupportedScheme => {
                "use an https:// or ssh:// address, git@host:path, or owner/repo"
            }
            Self::Credentials => {
                "the repository address must not contain a password or token; use your Git credential manager"
            }
            Self::BadHost => "the repository host name isn't valid",
            Self::LocalHost => {
                "addresses on this computer or a local network can't be used as an activity's source"
            }
            Self::SourceNotHttps => {
                "an activity's source must be an https:// address or owner/repo, not ssh"
            }
            Self::PrivateNotConfirmed => {
                "this address is on your own computer or network; confirm that before Lullshift uses it"
            }
            Self::SpecialAddress => {
                "this address can't be a Git server (it is link-local, multicast, broadcast or reserved)"
            }
            Self::BadPort => "the repository port isn't valid",
            Self::BadPath => "the repository path isn't valid",
            Self::BadShorthand => "owner/repo shorthand isn't valid",
        })
    }
}

/// Why a Git ref was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefError {
    Empty,
    TooLong,
    /// A leading `-`.
    LooksLikeOption,
    /// A character outside `A-Za-z0-9._/-`.
    ForbiddenCharacter,
    /// `..`, `//`, a leading or trailing `/`, a component starting with `.`, ending `.` or
    /// `.lock`.
    BadShape,
    /// `git check-ref-format` said no.
    Rejected,
}

impl fmt::Display for RefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "the ref is empty",
            Self::TooLong => "the ref is too long",
            Self::LooksLikeOption => "a ref can't start with '-'",
            Self::ForbiddenCharacter => "the ref has a character that isn't allowed",
            Self::BadShape => "the ref isn't a valid branch, tag or commit name",
            Self::Rejected => "Git doesn't accept this ref name",
        })
    }
}

/// Why a relative path was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    Empty,
    TooLong,
    TooDeep,
    /// An empty, `.` or `..` segment.
    BadSegment,
    /// A `.git` segment.
    GitDir,
    /// A segment starting with `-`.
    LooksLikeOption,
    /// A character outside `A-Za-z0-9._-`, a leading `.`, or a trailing `.`.
    ForbiddenCharacter,
    /// `CON`, `NUL`, `COM1` ... with or without an extension.
    WindowsReserved,
    /// Two paths that differ only by case.
    CaseCollision,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "the path is empty",
            Self::TooLong => "the path is too long",
            Self::TooDeep => "the path has too many folders",
            Self::BadSegment => "the path has an empty, '.' or '..' part",
            Self::GitDir => "the path has a .git part",
            Self::LooksLikeOption => "a path part can't start with '-'",
            Self::ForbiddenCharacter => "the path has a character that isn't allowed",
            Self::WindowsReserved => "the path uses a name Windows reserves",
            Self::CaseCollision => "two paths differ only by upper and lower case",
        })
    }
}

/// Why a marketplace name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Invalid,
    /// `lullshift` belongs to the built-in marketplace.
    Reserved,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "the marketplace name isn't valid",
            Self::Reserved => "the name 'lullshift' is reserved for the built-in marketplace",
        })
    }
}

/// An honest category for a failed `git` call (S08-T04). The raw stderr is never kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFailureKind {
    /// No network, DNS failure, connection refused or unreachable host.
    Offline,
    /// Credentials missing, wrong or not allowed to see this repository.
    AuthDenied,
    /// The repository, ref or path doesn't exist.
    NotFound,
    /// The call took too long and was stopped.
    Timeout,
    /// `git` isn't installed or isn't on `PATH`.
    GitMissing,
    /// A command's output was larger than allowed and the call was stopped.
    OutputTooLarge,
    /// Anything else.
    Failed,
}

impl fmt::Display for GitFailureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Offline => "can't reach the repository; check your network connection",
            Self::AuthDenied => {
                "access was denied, or the repository can't be seen with your Git credentials"
            }
            Self::NotFound => "the repository, branch or tag wasn't found",
            Self::Timeout => "Git took too long and was stopped",
            Self::GitMissing => "Git isn't installed or isn't on your PATH",
            Self::OutputTooLarge => "Git returned more data than allowed and was stopped",
            Self::Failed => "Git couldn't complete the request",
        })
    }
}

/// Why an index was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexError {
    Missing,
    NotARegularFile,
    TooLarge,
    /// Not JSON, wrong shape, unknown field or wrong type (sanitized parser message).
    Parse(String),
    UnsupportedSchemaVersion,
    TooManyEntries,
    DuplicateId(String),
    BadId(String),
    BadText(&'static str),
    Name(NameError),
    Entry {
        id: String,
        reason: String,
    },
}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("the repository has no .lullshift/marketplace.json"),
            Self::NotARegularFile => f.write_str("the marketplace index isn't a regular file"),
            Self::TooLarge => f.write_str("the marketplace index is too large"),
            Self::Parse(why) => write!(f, "the marketplace index isn't valid: {why}"),
            Self::UnsupportedSchemaVersion => {
                f.write_str("the marketplace index version is not supported")
            }
            Self::TooManyEntries => f.write_str("the marketplace index lists too many activities"),
            Self::DuplicateId(id) => write!(f, "the activity {id} is listed twice"),
            Self::BadId(id) => write!(f, "'{id}' isn't a valid activity id"),
            Self::BadText(which) => {
                write!(
                    f,
                    "the marketplace {which} isn't plain text of a valid length"
                )
            }
            Self::Name(e) => e.fmt(f),
            Self::Entry { id, reason } => write!(f, "the entry {id} isn't valid: {reason}"),
        }
    }
}

/// Why a pack was refused. Paths are sanitized for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    /// The folder isn't in the repository at that commit, or isn't a folder.
    NotFound,
    TooManyFiles,
    FileTooLarge(String),
    PackTooLarge,
    Symlink(String),
    Submodule(String),
    Executable(String),
    UnsupportedMode(String),
    BadPath(String, PathError),
    DisallowedFile(String),
    MissingManifest,
    ManifestInvalid(&'static str),
    IdMismatch {
        expected: String,
        found: String,
    },
    UnknownPermission(String),
    EngineMissing(String),
    EngineUnusable(String),
    DataMissing(String),
    DataTooLarge(String),
    DataNotJson(String),
    DataInvalid(Vec<String>),
    /// A pack image failed its checks, or the data names an image the manifest doesn't list.
    /// The text is already plain and cleaned (path and reason).
    ImageInvalid(String),
    /// An installed folder doesn't exist or isn't a plain folder of regular files.
    Unreadable,
    /// What `git` returned didn't match what it listed.
    Inconsistent,
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("the activity folder wasn't found at that commit"),
            Self::TooManyFiles => f.write_str("the activity has more than 512 files"),
            Self::FileTooLarge(p) => write!(f, "{p} is larger than 5 MiB"),
            Self::PackTooLarge => f.write_str("the activity is larger than 20 MiB"),
            Self::Symlink(p) => write!(f, "{p} is a symbolic link; links aren't allowed"),
            Self::Submodule(p) => write!(f, "{p} is a submodule; submodules aren't allowed"),
            Self::Executable(p) => {
                write!(f, "{p} is marked executable; executables aren't allowed")
            }
            Self::UnsupportedMode(p) => write!(f, "{p} isn't a regular file"),
            Self::BadPath(p, why) => write!(f, "{p}: {why}"),
            Self::DisallowedFile(p) => {
                write!(f, "{p} isn't a file type this kind of activity may contain")
            }
            Self::MissingManifest => f.write_str("the activity has no manifest.json"),
            Self::ManifestInvalid(why) => write!(f, "manifest.json isn't valid: {why}"),
            Self::IdMismatch { expected, found } => {
                write!(
                    f,
                    "the marketplace lists {expected} but manifest.json says {found}"
                )
            }
            Self::UnknownPermission(p) => {
                write!(f, "the activity asks for an unknown permission: {p}")
            }
            Self::EngineMissing(e) => write!(f, "this version of Lullshift has no {e} engine"),
            Self::EngineUnusable(e) => write!(f, "the {e} engine can't check this data"),
            Self::DataMissing(p) => write!(f, "{p} is missing"),
            Self::DataTooLarge(p) => write!(f, "{p} is over 256 KiB"),
            Self::DataNotJson(p) => write!(f, "{p} isn't valid JSON"),
            Self::DataInvalid(problems) => {
                write!(f, "the content isn't valid: {}", problems.join("; "))
            }
            Self::ImageInvalid(why) => write!(f, "a picture isn't allowed: {why}"),
            Self::Unreadable => f.write_str("the installed activity folder can't be read"),
            Self::Inconsistent => f.write_str("Git returned an unexpected object"),
        }
    }
}

/// The crate error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Url(UrlError),
    Ref(RefError),
    Path(PathError),
    Name(NameError),
    Git(GitFailureKind),
    Index(IndexError),
    Pack(PackError),
    /// A local file operation failed. Only the operation and the OS error kind are kept.
    Io {
        what: &'static str,
        kind: std::io::ErrorKind,
    },
    /// Something other than a pinned commit came back where a commit was required.
    NotACommit,
    /// The repository answered with a redirect to `to` (decision 18.1). Never followed; the
    /// user decides. `same_host`: same host name and port.
    Moved {
        to: Box<crate::validate::GitUrl>,
        same_host: bool,
    },
}

impl Error {
    pub(crate) fn io(what: &'static str) -> impl FnOnce(std::io::Error) -> Self {
        move |e| Self::Io {
            what,
            kind: e.kind(),
        }
    }

    /// The Git failure category, if this error is one.
    pub fn git_kind(&self) -> Option<GitFailureKind> {
        match self {
            Self::Git(k) => Some(*k),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(e) => e.fmt(f),
            Self::Ref(e) => e.fmt(f),
            Self::Path(e) => e.fmt(f),
            Self::Name(e) => e.fmt(f),
            Self::Git(e) => e.fmt(f),
            Self::Index(e) => e.fmt(f),
            Self::Pack(e) => e.fmt(f),
            Self::Io { what, kind } => write!(f, "couldn't {what}: {kind}"),
            Self::NotACommit => f.write_str("Git didn't return a commit"),
            Self::Moved { to, same_host } => {
                if *same_host {
                    write!(
                        f,
                        "the repository moved to {} on the same host",
                        to.display()
                    )
                } else {
                    write!(f, "the repository moved to another host: {}", to.display())
                }
            }
        }
    }
}

impl std::error::Error for Error {}

macro_rules! from {
    ($($t:ty => $v:ident),*) => {
        $(impl From<$t> for Error {
            fn from(e: $t) -> Self {
                Self::$v(e)
            }
        })*
    };
}
from!(
    UrlError => Url,
    RefError => Ref,
    PathError => Path,
    NameError => Name,
    GitFailureKind => Git,
    IndexError => Index,
    PackError => Pack
);
