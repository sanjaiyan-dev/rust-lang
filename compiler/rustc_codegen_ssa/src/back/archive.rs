use rustc_data_structures::fx::FxHashSet;
use rustc_data_structures::memmap::Mmap;
use rustc_session::cstore::DllImport;
use rustc_session::Session;
use rustc_span::symbol::Symbol;

use object::read::archive::ArchiveFile;

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use crate::errors::{ExtractBundledLibsError, ExtractBundledLibsErrorKind::*};

pub trait ArchiveBuilderBuilder {
    fn new_archive_builder<'a>(&self, sess: &'a Session) -> Box<dyn ArchiveBuilder<'a> + 'a>;

    /// Creates a DLL Import Library <https://docs.microsoft.com/en-us/windows/win32/dlls/dynamic-link-library-creation#creating-an-import-library>.
    /// and returns the path on disk to that import library.
    /// This functions doesn't take `self` so that it can be called from
    /// `linker_with_args`, which is specialized on `ArchiveBuilder` but
    /// doesn't take or create an instance of that type.
    fn create_dll_import_lib(
        &self,
        sess: &Session,
        lib_name: &str,
        dll_imports: &[DllImport],
        tmpdir: &Path,
        is_direct_dependency: bool,
    ) -> PathBuf;

    fn extract_bundled_libs<'a>(
        &'a self,
        rlib: &'a Path,
        outdir: &Path,
        bundled_lib_file_names: &FxHashSet<Symbol>,
    ) -> Result<(), ExtractBundledLibsError<'_>> {
        let archive_map = unsafe {
            Mmap::map(File::open(rlib).map_err(|e| ExtractBundledLibsError {
                kind: OpenFile,
                rlib,
                error: e.to_string(),
            })?)
            .map_err(|e| ExtractBundledLibsError {
                kind: MmapFile,
                rlib,
                error: e.to_string(),
            })?
        };
        let archive = ArchiveFile::parse(&*archive_map).map_err(|e| ExtractBundledLibsError {
            kind: ParseArchive,
            rlib,
            error: e.to_string(),
        })?;

        for entry in archive.members() {
            let entry = entry.map_err(|e| ExtractBundledLibsError {
                kind: ReadEntry,
                rlib,
                error: e.to_string(),
            })?;
            let data = entry.data(&*archive_map).map_err(|e| ExtractBundledLibsError {
                kind: ArchiveMember,
                rlib,
                error: e.to_string(),
            })?;
            let name = std::str::from_utf8(entry.name()).map_err(|e| ExtractBundledLibsError {
                kind: ConvertName,
                rlib,
                error: e.to_string(),
            })?;
            if !bundled_lib_file_names.contains(&Symbol::intern(name)) {
                continue; // We need to extract only native libraries.
            }
            std::fs::write(&outdir.join(&name), data).map_err(|e| ExtractBundledLibsError {
                kind: WriteFile,
                rlib,
                error: e.to_string(),
            })?;
        }
        Ok(())
    }
}

pub trait ArchiveBuilder<'a> {
    fn add_file(&mut self, path: &Path);

    fn add_archive(
        &mut self,
        archive: &Path,
        skip: Box<dyn FnMut(&str) -> bool + 'static>,
    ) -> io::Result<()>;

    fn build(self: Box<Self>, output: &Path) -> bool;
}
