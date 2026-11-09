//! An open project.

use std::path::{Path, PathBuf};
use time::OffsetDateTime;
use tronic_core::Project;
use tronic_pcb::sync::SyncReport;
use tronic_pcb::{Applied as PcbApplied, Board, FootprintDef, PcbCommand, PcbEditor, PcbError};
use tronic_schematic::{
    Applied, Connectivity, EditCommand, EditError, Editor, Netlist, Schematic, SymbolDef,
};

use crate::format::{self, sha256_hex};
use crate::{store, ProjectError};

/// How a save treats changes made to the file by someone else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    /// Fails with [`ProjectError::Conflict`] if the file changed on disk.
    IfUnchanged,
    /// Replaces the file after the user explicitly agreed. The replaced
    /// version remains available as backup.
    Overwrite,
}

/// A project being edited, bound to a file.
#[derive(Debug)]
pub struct ProjectSession {
    project: Project,
    editor: Editor,
    pcb: PcbEditor,
    path: PathBuf,
    /// Hash of the file content this session last read or wrote.
    disk_hash: String,
    dirty: bool,
    /// Counts changes, so that a snapshot is written only for new ones.
    revision: u64,
    autosaved_revision: Option<u64>,
}

impl ProjectSession {
    /// Creates a project file. Never replaces an existing file.
    pub fn create(path: &Path, name: &str, description: &str) -> Result<Self, ProjectError> {
        let path = format::with_extension(path);
        if path.exists() {
            return Err(ProjectError::AlreadyExists { path });
        }
        let project = Project::new(name, description, OffsetDateTime::now_utc())?;
        let schematic = Schematic::default();
        let bytes = format::encode(&project, &schematic, &Board::default(), &path)?;
        store::write_atomic(&path, &bytes)?;
        Ok(Self {
            project,
            editor: Editor::new(schematic),
            pcb: PcbEditor::default(),
            path,
            disk_hash: sha256_hex(&bytes),
            dirty: false,
            revision: 0,
            autosaved_revision: None,
        })
    }

    /// Opens and verifies an existing project file.
    pub fn open(path: &Path) -> Result<Self, ProjectError> {
        let bytes = store::read_file(path)?;
        let document = format::decode(&bytes, path)?;
        Ok(Self {
            project: document.project,
            editor: Editor::new(document.schematic),
            pcb: PcbEditor::new(document.board),
            path: path.to_owned(),
            disk_hash: sha256_hex(&bytes),
            dirty: false,
            revision: 0,
            autosaved_revision: None,
        })
    }

    /// Continues work from a snapshot. It counts as unsaved, and the recorded
    /// file hash keeps external changes from being overwritten unnoticed.
    pub(crate) fn recovered(document: format::Document, path: PathBuf, disk_hash: String) -> Self {
        Self {
            project: document.project,
            editor: Editor::new(document.schematic),
            pcb: PcbEditor::new(document.board),
            path,
            disk_hash,
            dirty: true,
            revision: 1,
            autosaved_revision: None,
        }
    }

    pub(crate) fn disk_hash(&self) -> &str {
        &self.disk_hash
    }

    /// Whether there are unsaved changes newer than the last snapshot.
    pub fn needs_autosave(&self) -> bool {
        self.dirty && self.autosaved_revision != Some(self.revision)
    }

    pub(crate) fn mark_autosaved(&mut self) {
        self.autosaved_revision = Some(self.revision);
    }

    fn mark_changed(&mut self) {
        self.dirty = true;
        self.revision += 1;
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn schematic(&self) -> &Schematic {
        self.editor.doc()
    }

    pub fn board(&self) -> &Board {
        self.pcb.board()
    }

    pub fn pcb_can_undo(&self) -> bool {
        self.pcb.can_undo()
    }

    pub fn pcb_can_redo(&self) -> bool {
        self.pcb.can_redo()
    }

    /// The netlist of the current schematic.
    pub fn netlist(&self) -> Netlist {
        let doc = self.editor.doc();
        Netlist::build(doc, &Connectivity::analyze(doc))
    }

    /// Runs an edit command on the board.
    pub fn apply_pcb(&mut self, command: PcbCommand) -> Result<PcbApplied, PcbError> {
        let (applied, changed) = self.pcb.apply(command)?;
        if changed {
            self.mark_changed();
        }
        Ok(applied)
    }

    pub fn undo_pcb(&mut self) -> Result<(), PcbError> {
        self.pcb.undo()?;
        self.mark_changed();
        Ok(())
    }

    pub fn redo_pcb(&mut self) -> Result<(), PcbError> {
        self.pcb.redo()?;
        self.mark_changed();
        Ok(())
    }

    /// Brings the board in line with the schematic.
    pub fn sync_board(
        &mut self,
        find_footprint: &dyn Fn(&str) -> Option<FootprintDef>,
    ) -> SyncReport {
        let netlist = self.netlist();
        let (report, changed) = self.pcb.sync(&netlist, find_footprint);
        if changed {
            self.mark_changed();
        }
        report
    }

    pub fn can_undo(&self) -> bool {
        self.editor.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.editor.can_redo()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Updates project metadata in memory. On error nothing changes.
    pub fn update_metadata(&mut self, name: &str, description: &str) -> Result<(), ProjectError> {
        if self.project.set_metadata(name, description)? {
            self.mark_changed();
        }
        Ok(())
    }

    /// Runs an edit command on the schematic.
    pub fn apply(&mut self, command: EditCommand) -> Result<Applied, EditError> {
        let revision_before = self.editor.doc().clone();
        let applied = self.editor.apply(command)?;
        if *self.editor.doc() != revision_before {
            self.mark_changed();
        }
        Ok(applied)
    }

    /// Takes over a reviewed set of changes as one undo step.
    pub fn commit_schematic(&mut self, doc: Schematic) {
        self.editor.commit(doc);
        self.mark_changed();
    }

    pub fn undo(&mut self) -> Result<(), EditError> {
        self.editor.undo()?;
        self.mark_changed();
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), EditError> {
        self.editor.redo()?;
        self.mark_changed();
        Ok(())
    }

    /// Embeds symbols in the project, which keeps it readable on its own.
    pub fn add_symbols(&mut self, symbols: impl IntoIterator<Item = SymbolDef>) {
        self.editor.add_symbols(symbols);
        self.mark_changed();
    }

    /// Writes the project to its file.
    pub fn save(&mut self, mode: SaveMode) -> Result<(), ProjectError> {
        if mode == SaveMode::IfUnchanged {
            // A deleted file is no conflict: writing it loses nothing.
            if let Some(current) = store::read_if_exists(&self.path)? {
                if sha256_hex(&current) != self.disk_hash {
                    return Err(ProjectError::Conflict {
                        path: self.path.clone(),
                    });
                }
            }
        }
        let path = self.path.clone();
        self.write_to(&path)
    }

    /// Writes the project to a new file and continues working on that file.
    /// Callers confirm replacing an existing target beforehand.
    pub fn save_as(&mut self, path: &Path) -> Result<(), ProjectError> {
        self.write_to(&format::with_extension(path))
    }

    fn write_to(&mut self, path: &Path) -> Result<(), ProjectError> {
        let mut updated = self.project.clone();
        updated.modified_at = OffsetDateTime::now_utc();
        let bytes = format::encode(&updated, self.editor.doc(), self.pcb.board(), path)?;
        store::write_atomic(path, &bytes)?;
        // State changes only after the file is safely written.
        self.project = updated;
        self.path = path.to_owned();
        self.disk_hash = sha256_hex(&bytes);
        self.dirty = false;
        Ok(())
    }
}
