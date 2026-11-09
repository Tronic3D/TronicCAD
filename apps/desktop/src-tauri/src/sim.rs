//! Tauri commands of the circuit simulation and the 3D view.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tauri::State;
use tronic_3d::{build_scene, parse_model, Scene};
use tronic_pcb::{ImportedModel, PcbCommand};
use tronic_project::store;
use tronic_schematic::Schematic;
use tronic_sim::{
    circuit_from_schematic, parse_value, simulate, Analysis, BuildReport, Circuit, SimResult,
};

use crate::error::CommandError;
use crate::AppState;

/// Largest model file read, which keeps a wrong file from filling the memory.
const MAX_MODEL_BYTES: u64 = 256 * 1024 * 1024;

/// Settings as the user typed them, like `10k` or `1u`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnalysisRequest {
    OperatingPoint,
    DcSweep {
        source: String,
        start: String,
        stop: String,
        step: String,
    },
    Ac {
        start: String,
        stop: String,
        points_per_decade: u32,
    },
    Transient {
        stop: String,
        step: String,
    },
}

fn number(text: &str) -> Result<f64, CommandError> {
    parse_value(text).ok_or_else(|| CommandError::new("sim.invalidValue").with("value", text))
}

fn analysis(request: &AnalysisRequest) -> Result<Analysis, CommandError> {
    Ok(match request {
        AnalysisRequest::OperatingPoint => Analysis::OperatingPoint,
        AnalysisRequest::DcSweep {
            source,
            start,
            stop,
            step,
        } => Analysis::DcSweep {
            source: source.clone(),
            start: number(start)?,
            stop: number(stop)?,
            step: number(step)?,
        },
        AnalysisRequest::Ac {
            start,
            stop,
            points_per_decade,
        } => Analysis::Ac {
            start: number(start)?,
            stop: number(stop)?,
            points_per_decade: *points_per_decade,
        },
        AnalysisRequest::Transient { stop, step } => Analysis::Transient {
            stop: number(stop)?,
            step: number(step)?,
        },
    })
}

fn supplies(texts: &BTreeMap<String, String>) -> Result<BTreeMap<String, f64>, CommandError> {
    texts
        .iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(net, text)| Ok((net.clone(), number(text)?)))
        .collect()
}

#[derive(Serialize)]
pub struct SimPreparation {
    report: BuildReport,
    complete: bool,
    /// The equivalent SPICE deck, so that every result can be checked elsewhere.
    deck: String,
    /// Names usable as the swept source.
    sources: Vec<String>,
}

fn prepare(
    doc: &Schematic,
    name: &str,
    voltages: &BTreeMap<String, f64>,
) -> (Circuit, SimPreparation) {
    let (circuit, report) = circuit_from_schematic(doc, voltages);
    let sources = circuit
        .devices
        .iter()
        .filter(|device| {
            matches!(
                device.element,
                tronic_sim::Element::Voltage { .. } | tronic_sim::Element::Current { .. }
            )
        })
        .map(|device| device.name.clone())
        .collect();
    let preparation = SimPreparation {
        complete: report.is_complete(),
        deck: circuit.to_spice(name),
        report,
        sources,
    };
    (circuit, preparation)
}

/// Shows how the schematic would be simulated, without running anything.
#[tauri::command(async)]
pub fn sim_prepare(
    state: State<'_, AppState>,
    supplies: BTreeMap<String, String>,
) -> Result<SimPreparation, CommandError> {
    let voltages = self::supplies(&supplies)?;
    state.with_session(|session| {
        let name = session.project().name.clone();
        Ok(prepare(session.schematic(), &name, &voltages).1)
    })
}

#[derive(Serialize)]
pub struct SimRun {
    preparation: SimPreparation,
    result: SimResult,
}

/// Runs outside the session lock, so that the application stays responsive.
#[tauri::command(async)]
pub fn sim_run(
    state: State<'_, AppState>,
    supplies: BTreeMap<String, String>,
    analysis: AnalysisRequest,
) -> Result<SimRun, CommandError> {
    let voltages = self::supplies(&supplies)?;
    let settings = self::analysis(&analysis)?;
    let (doc, name) = state.with_session(|session| {
        Ok((session.schematic().clone(), session.project().name.clone()))
    })?;
    let (circuit, preparation) = prepare(&doc, &name, &voltages);
    // Without a value or a supply voltage the result would be meaningless.
    if !preparation.report.bad_values.is_empty() || !preparation.report.unknown_supplies.is_empty()
    {
        return Err(CommandError::new("sim.incomplete"));
    }
    let result = simulate(&circuit, &settings)?;
    Ok(SimRun {
        preparation,
        result,
    })
}

/// Writes a result or a deck chosen for saving by the user.
#[tauri::command(async)]
pub fn sim_save_text(path: PathBuf, text: String) -> Result<(), CommandError> {
    store::replace_atomic(&path, text.as_bytes())?;
    Ok(())
}

#[tauri::command(async)]
pub fn view3d_scene(state: State<'_, AppState>) -> Result<Scene, CommandError> {
    state.with_session(|session| Ok(build_scene(session.board())))
}

/// Reads a model file and uses it for every part with this footprint.
#[tauri::command(async)]
pub fn view3d_import_model(
    state: State<'_, AppState>,
    footprint: String,
    path: PathBuf,
    scale: f32,
    z_offset: f32,
) -> Result<Scene, CommandError> {
    if !(scale.is_finite() && scale > 0.0 && z_offset.is_finite()) {
        return Err(CommandError::new("model.invalidPlacement"));
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_owned();
    let read_error = |source: std::io::Error| {
        CommandError::new("model.io")
            .with("path", path.display())
            .with("detail", source)
    };
    let size = std::fs::metadata(&path).map_err(read_error)?.len();
    if size > MAX_MODEL_BYTES {
        return Err(tronic_3d::ImportError::TooLarge.into());
    }
    let bytes = std::fs::read(&path).map_err(read_error)?;
    let mesh = parse_model(&extension, &bytes)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    state.with_session(|session| {
        session.apply_pcb(PcbCommand::SetFootprintModel {
            footprint,
            model: Some(ImportedModel {
                name,
                positions: mesh.positions,
                scale,
                z_offset,
            }),
        })?;
        Ok(build_scene(session.board()))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn reads_settings_as_typed() {
        let request = AnalysisRequest::Transient {
            stop: "2m".to_owned(),
            step: "1u".to_owned(),
        };
        assert_eq!(
            analysis(&request).unwrap(),
            Analysis::Transient {
                stop: 2e-3,
                step: 1e-6
            }
        );
    }

    #[test]
    fn names_the_value_it_cannot_read() {
        let request = AnalysisRequest::Transient {
            stop: "soon".to_owned(),
            step: "1u".to_owned(),
        };
        let error = analysis(&request).unwrap_err();
        assert_eq!(error.code, "sim.invalidValue");
        assert_eq!(error.params["value"], "soon");
    }

    #[test]
    fn empty_supply_fields_count_as_not_given() {
        let texts = BTreeMap::from([
            ("VIN".to_owned(), "9".to_owned()),
            ("VREF".to_owned(), "  ".to_owned()),
        ]);
        assert_eq!(
            supplies(&texts).unwrap(),
            BTreeMap::from([("VIN".to_owned(), 9.0)])
        );
    }
}
