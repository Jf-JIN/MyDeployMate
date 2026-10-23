//! Small JSON files replaced atomically: write a temp file next to the target, flush it, rename.

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::error::{StoreError, io};
use crate::workspace::retry;

pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), StoreError> {
    let mut bytes = serde_json::to_vec_pretty(value).expect("store files serialize");
    bytes.push(b'\n');
    let dir = path.parent().expect("store files have a parent");
    fs::create_dir_all(dir).map_err(io(dir))?;
    let tmp = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut f = fs::File::create(&tmp).map_err(io(&tmp))?;
        f.write_all(&bytes).map_err(io(&tmp))?;
        f.sync_all().map_err(io(&tmp))?;
    }
    retry(|| fs::rename(&tmp, path)).map_err(io(path))
}
