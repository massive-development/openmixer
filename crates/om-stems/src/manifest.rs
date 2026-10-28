//! Describes a downloadable model (for example Demucs ONNX weights): where to fetch it, how to verify
//! it and the licence the user must accept first. Weights are
//! never committed to the repository; manifests are plain JSON so users and plugins can point at other
//! models or mirrors.

use crate::contract::ModelContract;
use serde_json::{json, Value};
use thiserror::Error;

/// Licence name for the htdemucs weights.
pub const HTDEMUCS_LICENSE_NAME: &str = "MIT (Demucs code and weights); training data not covered";

/// Shown in the licence dialog before htdemucs is downloaded (S4 wires the UI). Keep in step with
/// `LICENSE_TEXT` in `tools/export_htdemucs.py` and the row in `docs/legal/asset-provenance.csv`.
pub const HTDEMUCS_LICENSE_TEXT: &str = "\
htdemucs is Demucs by Meta Platforms, Inc. and affiliates. The Demucs code and the released \
weights are published under the MIT licence (see the LICENSE file of the Demucs repository), which \
permits use, copying, modification and distribution of the software and the weights, provided the \
copyright and permission notice is kept.

Important: the MIT licence covers the code and the weights as published. It does not grant any \
rights in the data the model was trained on. The training set includes MUSDB18-HQ, which is \
distributed for research and academic use under its own terms. Those terms are not changed by the \
MIT licence of the weights. If you intend to use separated stems commercially, check that your use \
is acceptable for yourself. OpenMixer downloads the model to your computer only after you accept \
this notice, and never uploads your music.";

#[derive(Debug, Clone, PartialEq)]
pub struct ModelManifest {
    /// Also the file name in the model folder. Letters, digits, `.`, `_` and `-`; no leading dot.
    pub id: String,
    pub url: String,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    /// 0 = unknown.
    pub size_bytes: u64,
    /// For example `"MIT"`.
    pub license_name: String,
    /// Shown to the user before downloading.
    pub license_text: String,
    /// How the pipeline talks to the model; `None` means [`ModelContract::htdemucs`].
    pub contract: Option<ModelContract>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("manifest is not a JSON object")]
    NotAnObject,
    #[error("invalid model id")]
    InvalidId,
    #[error("missing url")]
    MissingUrl,
    #[error("sha256 must be 64 hex digits")]
    InvalidSha256,
    #[error("licence name and text are required")]
    MissingLicence,
    #[error("invalid model contract: {0}")]
    InvalidContract(String),
}

fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('.')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn is_hex_digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn text_field(object: &Value, key: &str) -> String {
    object[key].as_str().unwrap_or_default().to_owned()
}

impl ModelManifest {
    /// Parses and validates a manifest. The digest is lowercased before it is checked.
    pub fn from_json(json: &str) -> Result<Self, ManifestError> {
        let value: Value =
            serde_json::from_str(json).map_err(|e| ManifestError::Json(e.to_string()))?;
        if !value.is_object() {
            return Err(ManifestError::NotAnObject);
        }
        let manifest = Self {
            id: text_field(&value, "id"),
            url: text_field(&value, "url"),
            sha256: text_field(&value, "sha256").to_ascii_lowercase(),
            size_bytes: value["sizeBytes"].as_u64().unwrap_or(0),
            license_name: text_field(&value, "licenseName"),
            license_text: text_field(&value, "licenseText"),
            contract: match value.get("contract").filter(|c| !c.is_null()) {
                None => None,
                Some(c) => {
                    Some(ModelContract::from_json(c).map_err(ManifestError::InvalidContract)?)
                }
            },
        };
        if !is_safe_id(&manifest.id) {
            return Err(ManifestError::InvalidId);
        }
        if manifest.url.is_empty() {
            return Err(ManifestError::MissingUrl);
        }
        if !is_hex_digest(&manifest.sha256) {
            return Err(ManifestError::InvalidSha256);
        }
        if manifest.license_name.is_empty() || manifest.license_text.is_empty() {
            return Err(ManifestError::MissingLicence);
        }
        Ok(manifest)
    }

    /// A manifest for an htdemucs ONNX exported by `tools/export_htdemucs.py`, with the licence text
    /// the dialog shows. `url` and `sha256` come from the exporter's output.
    pub fn htdemucs(url: &str, sha256: &str, size_bytes: u64) -> Self {
        Self {
            id: "htdemucs.onnx".into(),
            url: url.into(),
            sha256: sha256.to_ascii_lowercase(),
            size_bytes,
            license_name: HTDEMUCS_LICENSE_NAME.into(),
            license_text: HTDEMUCS_LICENSE_TEXT.into(),
            contract: Some(ModelContract::htdemucs_split()),
        }
    }

    /// The contract to run the model with.
    pub fn contract_or_default(&self) -> ModelContract {
        self.contract.clone().unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        let mut value = json!({
            "id": self.id,
            "url": self.url,
            "sha256": self.sha256,
            "sizeBytes": self.size_bytes,
            "licenseName": self.license_name,
            "licenseText": self.license_text,
        });
        if let Some(contract) = &self.contract {
            value["contract"] = contract.to_json();
        }
        value.to_string()
    }
}
