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

    /// Global rclone flags that have no dedicated rc parameter, passed as the
    /// `_config` blob (https://rclone.org/rc/#setting-config-flags-with-config).
    /// Keys are rclone's internal option names, e.g. `DryRun` for `--dry-run`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "_config")]
    pub config: Option<serde_json::Map<String, serde_json::Value>>,
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

/// Builders whose request carries the generic `_config` blob. Bisync is
/// deliberately not one of them: it has dedicated parameters (`dryRun`, ...)
/// for what the others can only express through `_config`.
pub trait Configurable: Builder<Base = RequestBase> {
    /// Sets one rclone internal option name in the `_config` blob.
    fn set_config(&mut self, key: &str, value: serde_json::Value) {
        self.base_mut()
            .config
            .get_or_insert_with(serde_json::Map::new)
            .insert(key.to_string(), value);
    }

    /// `--dry-run`.
    fn dry_run(mut self, dry_run: bool) -> Self {
        if dry_run {
            self.set_config("DryRun", serde_json::Value::Bool(true));
        }
        self
    }

    /// `--update`: skip files that are newer on the destination.
    fn update_older(mut self, update: bool) -> Self {
        if update {
            self.set_config("UpdateOlder", serde_json::Value::Bool(true));
        }
        self
    }
}

impl Configurable for CopyBuilder {}
impl Configurable for SyncBuilder {}
impl Configurable for MoveBuilder {}

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

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createEmptySrcDirs")]
    pub create_empty_src_dirs: Option<bool>,
}

pub struct BiSyncBuilder {
    base: BiSyncRequestBase,
    resync: Option<bool>,
    force: Option<bool>,
    resync_mode: Option<String>,
    workdir: Option<String>,
    dry_run: Option<bool>,
    check_access: Option<bool>,
    create_empty_src_dirs: Option<bool>,
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
            create_empty_src_dirs: self.create_empty_src_dirs,
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
            create_empty_src_dirs: None,
        }
    }

    // The setters below only emit a parameter when it is switched on -
    // rclone's own default for each of them is off, so sending an explicit
    // `false` would only add noise to the request.

    pub fn resync(mut self, resync: bool) -> Self {
        self.resync = resync.then_some(true);
        self
    }

    pub fn create_empty_src_dirs(mut self, create: bool) -> Self {
        self.create_empty_src_dirs = create.then_some(true);
        self
    }

    pub fn workdir(mut self, workdir: String) -> Self {
        self.workdir = Some(workdir);
        self
    }

    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run.then_some(true);
        self
    }

    pub fn check_access(mut self, check_access: bool) -> Self {
        self.check_access = check_access.then_some(true);
        self
    }
}

// ============= COPY =============
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Copy {
    #[serde(flatten)]
    pub base: RequestBase,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createEmptySrcDirs")]
    pub create_empty_src_dirs: Option<bool>,
}

pub struct CopyBuilder {
    base: RequestBase,
    create_empty_src_dirs: Option<bool>,
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
            create_empty_src_dirs: self.create_empty_src_dirs,
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
                config: None,
            },
            create_empty_src_dirs: None,
        }
    }

    pub fn create_empty_src_dirs(mut self, create: bool) -> Self {
        self.create_empty_src_dirs = Some(create);
        self
    }
}

// ============= SYNC =============
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sync {
    #[serde(flatten)]
    pub base: RequestBase,

    // sync/sync has no deleteEmptySrcDirs - only sync/move does.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createEmptySrcDirs")]
    pub create_empty_src_dirs: Option<bool>,
}

pub struct SyncBuilder {
    base: RequestBase,
    create_empty_src_dirs: Option<bool>,
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
            create_empty_src_dirs: self.create_empty_src_dirs,
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
                config: None,
            },
            create_empty_src_dirs: None,
        }
    }

    pub fn create_empty_src_dirs(mut self, create: bool) -> Self {
        self.create_empty_src_dirs = Some(create);
        self
    }
}

// ============= MOVE =============
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Move {
    #[serde(flatten)]
    pub base: RequestBase,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createEmptySrcDirs")]
    pub create_empty_src_dirs: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "deleteEmptySrcDirs")]
    pub delete_empty_src_dirs: Option<bool>,
}

pub struct MoveBuilder {
    base: RequestBase,
    create_empty_src_dirs: Option<bool>,
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
            create_empty_src_dirs: self.create_empty_src_dirs,
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
                config: None,
            },
            create_empty_src_dirs: None,
            delete_empty_src_dirs: None,
        }
    }

    pub fn create_empty_src_dirs(mut self, create: bool) -> Self {
        self.create_empty_src_dirs = Some(create);
        self
    }

    pub fn delete_empty_src_dirs(mut self, delete: bool) -> Self {
        self.delete_empty_src_dirs = Some(delete);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bisync has a dedicated `dryRun` parameter.
    #[test]
    fn bisync_serializes_dry_run_as_a_parameter() {
        let req = BiSyncBuilder::new("/src".to_string(), "/dst".to_string())
            .dry_run(true)
            .check_access(true)
            .build();
        let json = serde_json::to_value(req).unwrap();
        assert_eq!(json["dryRun"], serde_json::json!(true));
        assert_eq!(json["checkAccess"], serde_json::json!(true));
        assert!(json.get("_config").is_none());
    }

    /// The other operations have no dry-run parameter, so it has to travel in
    /// the `_config` blob under rclone's internal option name.
    #[test]
    fn copy_serializes_dry_run_in_config_blob() {
        let req = CopyBuilder::new("/src".to_string(), "/dst".to_string())
            .dry_run(true)
            .update_older(true)
            .build();
        let json = serde_json::to_value(req).unwrap();
        assert_eq!(json["_config"]["DryRun"], serde_json::json!(true));
        assert_eq!(json["_config"]["UpdateOlder"], serde_json::json!(true));
    }

    /// Unset options must be omitted entirely rather than sent as `false`,
    /// so rclone keeps its own defaults.
    #[test]
    fn unset_options_are_omitted() {
        let req = MoveBuilder::new("/src".to_string(), "/dst".to_string())
            .dry_run(false)
            .delete_empty_src_dirs(true)
            .build();
        let json = serde_json::to_value(req).unwrap();
        assert_eq!(json["deleteEmptySrcDirs"], serde_json::json!(true));
        assert!(json.get("createEmptySrcDirs").is_none());
        assert!(json.get("_config").is_none());
    }
}
