use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterRules {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "IncludeRule")]
    pub include: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "ExcludeRule")]
    pub exclude: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestBase {
    #[serde(rename = "srcFs")]
    pub src_fs: String,

    #[serde(rename = "dstFs")]
    pub dst_fs: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "_filter")]
    pub filter: Option<FilterRules>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "_async")]
    pub async_op: Option<bool>,
}

// BiSync uses different parameter names: path1 and path2 instead of srcFs/dstFs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiSyncRequestBase {
    pub path1: String,
    pub path2: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "_filter")]
    pub filter: Option<FilterRules>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "_async")]
    pub async_op: Option<bool>,
}

// Trait to allow the Builder trait to modify filter and async_op generically
pub trait Filterable {
    fn filter_mut(&mut self) -> &mut Option<FilterRules>;
    fn async_op_mut(&mut self) -> &mut Option<bool>;
}

impl Filterable for RequestBase {
    fn filter_mut(&mut self) -> &mut Option<FilterRules> {
        &mut self.filter
    }
    fn async_op_mut(&mut self) -> &mut Option<bool> {
        &mut self.async_op
    }
}

impl Filterable for BiSyncRequestBase {
    fn filter_mut(&mut self) -> &mut Option<FilterRules> {
        &mut self.filter
    }
    fn async_op_mut(&mut self) -> &mut Option<bool> {
        &mut self.async_op
    }
}

/// Trait for builders to implement
pub trait Builder: Sized {
    type Output;
    type Base: Filterable;

    fn base_mut(&mut self) -> &mut Self::Base;
    fn build(self) -> Self::Output;

    // Common methods via trait
    fn sync(mut self) -> Self {
        *self.base_mut().async_op_mut() = Some(false);
        self
    }

    fn exclude(mut self, exclude: Vec<String>) -> Self {
        let filter = self.base_mut().filter_mut();
        if let Some(f) = filter {
            f.exclude = Some(exclude);
        } else {
            *filter = Some(FilterRules {
                include: None,
                exclude: Some(exclude),
            });
        }
        self
    }

    fn include(mut self, include: Vec<String>) -> Self {
        let filter = self.base_mut().filter_mut();
        if let Some(f) = filter {
            f.include = Some(include);
        } else {
            *filter = Some(FilterRules {
                include: Some(include),
                exclude: None,
            });
        }
        self
    }
}

// ============= BISYNC =============
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiSync {
    #[serde(flatten)]
    pub base: BiSyncRequestBase,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub resync: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,

    /// resyncMode - (string) During resync, prefer the version that is: path1, path2, newer, older, larger, smaller (default: path1 if --resync, otherwise none for no resync.)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "resyncMode")]
    pub resync_mode: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub workdir: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "dryRun")]
    pub dry_run: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "checkAccess")]
    pub check_access: Option<bool>,
}

pub struct BiSyncBuilder {
    base: BiSyncRequestBase,
    resync: Option<bool>,
    force: Option<bool>,
    resync_mode: Option<String>,
    workdir: Option<String>,
    dry_run: Option<bool>,
    check_access: Option<bool>,
}

impl Builder for BiSyncBuilder {
    type Output = BiSync;
    type Base = BiSyncRequestBase;

    fn base_mut(&mut self) -> &mut Self::Base {
        &mut self.base
    }

    fn build(self) -> BiSync {
        BiSync {
            base: self.base,
            resync: self.resync,
            force: self.force,
            resync_mode: self.resync_mode,
            workdir: self.workdir,
            dry_run: self.dry_run,
            check_access: self.check_access,
        }
    }
}

impl BiSyncBuilder {
    pub fn new(path1: String, path2: String) -> Self {
        Self {
            base: BiSyncRequestBase {
                path1,
                path2,
                filter: None,
                async_op: Some(true), // Default to true
            },
            resync: None,
            force: None,
            resync_mode: None,
            workdir: None,
            dry_run: None,
            check_access: None,
        }
    }

    pub fn resync(mut self, resync: bool) -> Self {
        self.resync = Some(resync);
        self
    }

    pub fn workdir(mut self, workdir: String) -> Self {
        self.workdir = Some(workdir);
        self
    }

    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = Some(dry_run);
        self
    }

    pub fn check_access(mut self, check_access: bool) -> Self {
        self.check_access = Some(check_access);
        self
    }
}

// ============= COPY =============
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Copy {
    #[serde(flatten)]
    pub base: RequestBase,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<bool>,
}

pub struct CopyBuilder {
    base: RequestBase,
    update: Option<bool>,
}

impl Builder for CopyBuilder {
    type Base = RequestBase;
    type Output = Copy;

    fn base_mut(&mut self) -> &mut Self::Base {
        &mut self.base
    }

    fn build(self) -> Copy {
        Copy {
            base: self.base,
            update: self.update,
        }
    }
}

impl CopyBuilder {
    pub fn new(src_fs: String, dst_fs: String) -> Self {
        Self {
            base: RequestBase {
                src_fs,
                dst_fs,
                filter: None,
                async_op: Some(true), // Default to true
            },
            update: None,
        }
    }

    pub fn update(mut self, update: bool) -> Self {
        self.update = Some(update);
        self
    }
}

// ============= SYNC =============
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sync {
    #[serde(flatten)]
    pub base: RequestBase,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_empty_src_dirs: Option<bool>,
}

pub struct SyncBuilder {
    base: RequestBase,
    delete_empty_src_dirs: Option<bool>,
}

impl Builder for SyncBuilder {
    type Output = Sync;
    type Base = RequestBase;

    fn base_mut(&mut self) -> &mut Self::Base {
        &mut self.base
    }

    fn build(self) -> Sync {
        Sync {
            base: self.base,
            delete_empty_src_dirs: self.delete_empty_src_dirs,
        }
    }
}

impl SyncBuilder {
    pub fn new(src_fs: String, dst_fs: String) -> Self {
        Self {
            base: RequestBase {
                src_fs,
                dst_fs,
                filter: None,
                async_op: Some(true), // Default to true
            },
            delete_empty_src_dirs: None,
        }
    }

    pub fn delete_empty_src_dirs(mut self, delete: bool) -> Self {
        self.delete_empty_src_dirs = Some(delete);
        self
    }
}

// ============= MOVE =============
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Move {
    #[serde(flatten)]
    pub base: RequestBase,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_empty_src_dirs: Option<bool>,
}

pub struct MoveBuilder {
    base: RequestBase,
    delete_empty_src_dirs: Option<bool>,
}

impl Builder for MoveBuilder {
    type Output = Move;
    type Base = RequestBase;

    fn base_mut(&mut self) -> &mut Self::Base {
        &mut self.base
    }

    fn build(self) -> Move {
        Move {
            base: self.base,
            delete_empty_src_dirs: self.delete_empty_src_dirs,
        }
    }
}

impl MoveBuilder {
    pub fn new(src_fs: String, dst_fs: String) -> Self {
        Self {
            base: RequestBase {
                src_fs,
                dst_fs,
                filter: None,
                async_op: Some(true), // Default to true
            },
            delete_empty_src_dirs: None,
        }
    }

    pub fn delete_empty_src_dirs(mut self, delete: bool) -> Self {
        self.delete_empty_src_dirs = Some(delete);
        self
    }
}
