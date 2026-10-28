//! Per-analysis editor state, shared by the input panel, report and status readout.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::app::store;
use crate::docs::{Registry, root_signal};
use bth_rigging::domain::SavedMat;
use bth_rigging::format::format_num;
use bth_rigging::mats::{MatBearingInput, MatBearingResult, calculate_mat_bearing};

#[derive(Clone, Copy)]
pub struct MatState {
    pub project_id: Uuid,
    pub project_name: Signal<String>,
    pub name: Signal<String>,
    pub mat_id: Signal<String>,
    pub outrigger_load: Signal<String>,
    pub pad_length: Signal<String>,
    pub pad_width: Signal<String>,
    pub allowable: Signal<String>,
    pub mats: Signal<Vec<SavedMat>>,
    pub status: Signal<String>,
    pub new_mat: NewMatForm,
}

/// Inline "+ New mat" form fields.
#[derive(Clone, Copy)]
pub struct NewMatForm {
    pub show: Signal<bool>,
    pub mfr: Signal<String>,
    pub model: Signal<String>,
    pub length: Signal<String>,
    pub width: Signal<String>,
    pub thickness: Signal<String>,
    pub weight: Signal<String>,
    pub allow_load: Signal<String>,
}

/// Results derived from the current inputs.
pub struct MatCalc {
    pub selected_mat: Option<SavedMat>,
    pub load: f64,
    pub pad_l: f64,
    pub pad_w: f64,
    pub allow: f64,
    pub result: Option<MatBearingResult>,
}

thread_local! {
    static MATS: Registry<MatState> = Registry::new();
}

pub fn state(analysis_id: Uuid) -> Result<MatState, String> {
    MATS.with(|mats| mats.get_or_load(analysis_id, || load(analysis_id)))
}

pub fn forget(analysis_id: Uuid) {
    MATS.with(|mats| mats.forget(analysis_id));
}

fn load(analysis_id: Uuid) -> Result<MatState, String> {
    let s = store().ok_or("Database not available.")?;
    let a = s
        .load_mat_analysis(analysis_id)
        .map_err(|e| e.to_string())?
        .ok_or("Mat analysis not found.")?;
    let project_name = s
        .load_project(a.project_id)
        .ok()
        .flatten()
        .map(|p| p.name)
        .unwrap_or_else(|| "Project".into());
    let mat_id = if a.mat_id.is_nil() { String::new() } else { a.mat_id.to_string() };
    Ok(MatState {
        project_id: a.project_id,
        project_name: root_signal(project_name),
        name: root_signal(a.name),
        mat_id: root_signal(mat_id),
        outrigger_load: root_signal(format_num(a.outrigger_load_lbs)),
        pad_length: root_signal(format_num(a.pad_length_in)),
        pad_width: root_signal(format_num(a.pad_width_in)),
        allowable: root_signal(format_num(a.allowable_psf)),
        mats: root_signal(s.list_mats().unwrap_or_default()),
        status: root_signal(String::new()),
        new_mat: NewMatForm {
            show: root_signal(false),
            mfr: root_signal(String::new()),
            model: root_signal(String::new()),
            length: root_signal("8".to_string()),
            width: root_signal("4".to_string()),
            thickness: root_signal("6".to_string()),
            weight: root_signal("0".to_string()),
            allow_load: root_signal("100000".to_string()),
        },
    })
}

impl MatState {
    pub fn derive(&self) -> MatCalc {
        let selected_mat = {
            let id_str = (self.mat_id)();
            (self.mats)().into_iter().find(|m| m.id.to_string() == id_str)
        };
        let load: f64 = (self.outrigger_load)().trim().parse().unwrap_or(f64::NAN);
        let pad_l: f64 = (self.pad_length)().trim().parse().unwrap_or(f64::NAN);
        let pad_w: f64 = (self.pad_width)().trim().parse().unwrap_or(f64::NAN);
        let allow: f64 = (self.allowable)().trim().parse().unwrap_or(f64::NAN);
        let result = selected_mat.as_ref().and_then(|mat| {
            calculate_mat_bearing(MatBearingInput::from_mat(mat, load, pad_l, pad_w, allow))
        });
        MatCalc {
            selected_mat,
            load,
            pad_l,
            pad_w,
            allow,
            result,
        }
    }
}
