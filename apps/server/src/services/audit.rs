//! AuditEvent handling.
//!
//! Emits FHIR `AuditEvent` resources for RESTful operations. These AuditEvents are stored in the
//! internal `audit_log` table (independent of the clinical `resources` store).
//!
//! Notes:
//! - Works with both FHIR R4/R4B and R5 (the AuditEvent shape differs).
//! - Emission is best-effort and must not fail the primary request path.

use crate::auth::Principal;
use crate::runtime_config::{ConfigKey, RuntimeConfigCache};
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct HttpAuditInput {
    pub method: String,
    pub interaction: String,
    pub action: String,
    pub status: u16,
    pub request_id: Option<String>,
    pub principal: Option<Principal>,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub target: Option<(String, String)>,
    pub patient_id: Option<String>,
    pub query_base64: Option<String>,
    pub query_harmonized: Option<String>,
    pub operation_outcome: Option<JsonValue>,
}

#[derive(Clone)]
pub struct AuditService {
    runtime_config_cache: std::sync::Arc<RuntimeConfigCache>,
    fhir_version: String,
    observer_display: String,
    sender: mpsc::Sender<AuditLogInsert>,
}

impl AuditService {
    pub fn new(
        runtime_config_cache: std::sync::Arc<RuntimeConfigCache>,
        fhir_version: String,
        observer_display: String,
        db_pool: PgPool,
    ) -> Self {
        let (sender, mut receiver) = mpsc::channel::<AuditLogInsert>(2048);

        tokio::spawn(async move {
            while let Some(row) = receiver.recv().await {
                if let Err(e) = insert_audit_log_row(&db_pool, &row).await {
                    tracing::warn!("Failed to persist audit_log row: {}", e);
                }
            }
        });

        tracing::info!("AuditEvent logging initialized (audit_log)");

        Self {
            runtime_config_cache,
            fhir_version,
            observer_display,
            sender,
        }
    }

    pub async fn enabled(&self) -> bool {
        self.runtime_config_cache.get(ConfigKey::AuditEnabled).await
    }

    pub async fn should_audit_interaction(&self, interaction: &str) -> bool {
        match interaction {
            "read" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsRead)
                    .await
            }
            "vread" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsVread)
                    .await
            }
            "history" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsHistory)
                    .await
            }
            "search" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsSearch)
                    .await
            }
            "create" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsCreate)
                    .await
            }
            "update" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsUpdate)
                    .await
            }
            "patch" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsPatch)
                    .await
            }
            "delete" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsDelete)
                    .await
            }
            "capabilities" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsCapabilities)
                    .await
            }
            "operation" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsOperation)
                    .await
            }
            "batch" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsBatch)
                    .await
            }
            "transaction" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsTransaction)
                    .await
            }
            "export" => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsExport)
                    .await
            }
            _ => {
                self.runtime_config_cache
                    .get(ConfigKey::AuditInteractionsOperation)
                    .await
            }
        }
    }

    pub async fn should_audit_status(&self, status: u16) -> bool {
        if status < 400 {
            return self
                .runtime_config_cache
                .get(ConfigKey::AuditIncludeSuccess)
                .await;
        }
        if status == 401 || status == 403 {
            return self
                .runtime_config_cache
                .get(ConfigKey::AuditIncludeAuthzFailure)
                .await;
        }
        self.runtime_config_cache
            .get(ConfigKey::AuditIncludeProcessingFailure)
            .await
    }

    pub async fn capture_search_query(&self) -> bool {
        self.runtime_config_cache
            .get(ConfigKey::AuditCaptureSearchQuery)
            .await
    }

    pub async fn capture_operation_outcome(&self) -> bool {
        self.runtime_config_cache
            .get(ConfigKey::AuditCaptureOperationOutcome)
            .await
    }

    pub async fn per_patient_events_for_search(&self) -> bool {
        self.runtime_config_cache
            .get(ConfigKey::AuditPerPatientEventsForSearch)
            .await
    }

    pub async fn enqueue_http(&self, input: HttpAuditInput) {
        if !self.enabled().await {
            return;
        }

        if !self.should_audit_interaction(&input.interaction).await
            || !self.should_audit_status(input.status).await
        {
            return;
        }

        let row = self.build_http_audit_row(input);
        match self.sender.try_send(row) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(row)) => {
                // Preserve request latency by deferring the await to a background task.
                let sender = self.sender.clone();
                tokio::spawn(async move {
                    if let Err(e) = sender.send(row).await {
                        tracing::warn!("Failed to enqueue audit event: {}", e);
                    }
                });
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_row)) => {
                tracing::warn!("Audit event queue closed; dropping event");
            }
        }
    }

    fn is_r5(&self) -> bool {
        self.fhir_version == "R5"
    }

    fn build_http_audit_row(&self, input: HttpAuditInput) -> AuditLogInsert {
        let principal = input.principal.clone();
        let (audit_event, fhir_action) = if self.is_r5() {
            (
                self.build_r5_http_event(input.clone()),
                input.action.clone(),
            )
        } else {
            (
                self.build_r4_http_event(input.clone()),
                input.action.clone(),
            )
        };
        let details = build_details(&input, principal.as_ref());

        let (resource_type, resource_id) = input
            .target
            .clone()
            .map(|(rt, id)| (Some(rt), Some(id)))
            .unwrap_or((None, None));

        let token_type = infer_token_type(principal.as_ref());

        let outcome = if input.status < 400 {
            "success".to_string()
        } else if input.status == 401 || input.status == 403 {
            "authz_failure".to_string()
        } else {
            "processing_failure".to_string()
        };

        AuditLogInsert {
            event_type: "fhir".to_string(),
            action: input.interaction,
            http_method: input.method,
            fhir_action,
            resource_type,
            resource_id,
            version_id: None,
            patient_id: input.patient_id,
            client_id: principal.as_ref().and_then(|p| p.client_id.clone()),
            user_id: principal.as_ref().map(|p| p.subject.clone()),
            scopes: principal.as_ref().map(|p| p.scopes.clone()),
            token_type,
            client_ip: input.client_ip,
            user_agent: input.user_agent,
            request_id: input.request_id,
            status_code: input.status as i32,
            outcome,
            audit_event,
            details,
        }
    }

    fn build_r5_http_event(&self, input: HttpAuditInput) -> JsonValue {
        let recorded = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);

        let (severity, outcome_code) = match input.status {
            0..=399 => ("informational", "information"),
            400..=499 => ("warning", "error"),
            _ => ("error", "fatal"),
        };

        let mut agents = vec![];

        // Client/user agent (requestor).
        let client_who = if let Some(p) = &input.principal {
            json!({
                "identifier": { "system": "urn:oidc:sub", "value": p.subject },
                "display": p.subject,
            })
        } else {
            json!({ "display": "Anonymous" })
        };

        let mut client_agent = json!({
            "who": client_who,
            "requestor": true,
            "role": [{
                "coding": [{
                    "system": "http://dicom.nema.org/resources/ontology/DCM",
                    "code": "110153",
                    "display": "Source Role ID"
                }]
            }],
        });

        if let Some(ip) = &input.client_ip {
            client_agent["networkString"] = json!(ip);
        }

        agents.push(client_agent);

        // Server agent (destination).
        agents.push(json!({
            "who": { "display": self.observer_display },
            "requestor": false,
            "role": [{
                "coding": [{
                    "system": "http://dicom.nema.org/resources/ontology/DCM",
                    "code": "110152",
                    "display": "Destination Role ID"
                }]
            }],
        }));

        let mut entity: Vec<JsonValue> = Vec::new();

        if let Some((rt, id)) = &input.target {
            entity.push(json!({
                "what": { "reference": format!("{}/{}", rt, id) },
                "description": format!("FHIR {} {} ({})", input.method, input.interaction, input.status),
                "detail": build_standard_entity_details(&input),
            }));
        } else {
            entity.push(json!({
                "description": format!("FHIR {} {} ({})", input.method, input.interaction, input.status),
                "detail": build_standard_entity_details(&input),
            }));
        }

        // Search query capture.
        if let Some(q) = &input.query_base64 {
            let mut query_entity = json!({
                "query": q,
                "description": input.query_harmonized.clone().unwrap_or_else(|| "FHIR search".to_string()),
            });
            if let Some(details) = build_standard_entity_details_opt(&input) {
                query_entity["detail"] = details;
            }
            entity.push(query_entity);
        }

        let mut contained: Vec<JsonValue> = Vec::new();
        if let Some(mut oo) = input.operation_outcome {
            if oo.get("id").and_then(|v| v.as_str()).is_none() {
                oo["id"] = json!("oo1");
            }
            contained.push(oo);
            entity.push(json!({
                "what": { "reference": "#oo1" },
                "description": "OperationOutcome from HTTP response",
            }));
        }

        let mut event = json!({
            "resourceType": "AuditEvent",
            "type": {
                "coding": [{
                    "system": "http://terminology.hl7.org/CodeSystem/audit-event-type",
                    "code": "rest"
                }]
            },
            "subtype": [{
                "coding": [{
                    "system": "http://hl7.org/fhir/restful-interaction",
                    "code": input.interaction
                }]
            }],
            "action": input.action,
            "severity": severity,
            "recorded": recorded,
            "outcome": {
                "code": {
                    "system": "http://terminology.hl7.org/CodeSystem/issue-severity",
                    "code": outcome_code
                }
            },
            "agent": agents,
            "source": {
                "observer": { "display": self.observer_display }
            },
            "entity": entity,
        });

        if let Some(pid) = input.patient_id {
            event["patient"] = json!({ "reference": format!("Patient/{}", pid) });
        }

        if !contained.is_empty() {
            event["contained"] = json!(contained);
        }

        event
    }

    fn build_r4_http_event(&self, input: HttpAuditInput) -> JsonValue {
        let recorded = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);

        // FHIR R4 AuditEvent.outcome uses 0|4|8|12 (success -> major failure).
        let outcome = match input.status {
            0..=399 => "0",
            400..=499 => "4",
            _ => "8",
        };

        let mut agents = vec![];

        let client_who = if let Some(p) = &input.principal {
            json!({
                "identifier": { "system": "urn:oidc:sub", "value": p.subject },
                "display": p.subject,
            })
        } else {
            json!({ "display": "Anonymous" })
        };

        let mut client_agent = json!({
            "who": client_who,
            "requestor": true,
            "role": [{
                "coding": [{
                    "system": "http://dicom.nema.org/resources/ontology/DCM",
                    "code": "110153",
                    "display": "Source Role ID"
                }]
            }],
        });

        if let Some(ip) = &input.client_ip {
            client_agent["network"] = json!({
                "address": ip,
                "type": "2"
            });
        }

        agents.push(client_agent);

        agents.push(json!({
            "who": { "display": self.observer_display },
            "requestor": false,
            "role": [{
                "coding": [{
                    "system": "http://dicom.nema.org/resources/ontology/DCM",
                    "code": "110152",
                    "display": "Destination Role ID"
                }]
            }],
        }));

        let mut entity: Vec<JsonValue> = Vec::new();

        if let Some((rt, id)) = &input.target {
            entity.push(json!({
                "what": { "reference": format!("{}/{}", rt, id) },
                "type": { "system": "http://hl7.org/fhir/resource-types", "code": rt },
                "description": format!("FHIR {} {} ({})", input.method, input.interaction, input.status),
                "detail": build_standard_entity_details(&input),
            }));
        } else {
            entity.push(json!({
                "description": format!("FHIR {} {} ({})", input.method, input.interaction, input.status),
                "detail": build_standard_entity_details(&input),
            }));
        }

        if let Some(q) = &input.query_base64 {
            let mut query_entity = json!({
                "query": q,
                "description": input.query_harmonized.clone().unwrap_or_else(|| "FHIR search".to_string()),
            });
            if let Some(details) = build_standard_entity_details_opt(&input) {
                query_entity["detail"] = details;
            }
            entity.push(query_entity);
        }

        let mut contained: Vec<JsonValue> = Vec::new();
        if let Some(mut oo) = input.operation_outcome {
            if oo.get("id").and_then(|v| v.as_str()).is_none() {
                oo["id"] = json!("oo1");
            }
            contained.push(oo);
            entity.push(json!({
                "what": { "reference": "#oo1" },
                "type": { "system": "http://hl7.org/fhir/resource-types", "code": "OperationOutcome" },
                "description": "OperationOutcome from HTTP response",
            }));
        }

        let mut event = json!({
            "resourceType": "AuditEvent",
            "type": {
                "system": "http://terminology.hl7.org/CodeSystem/audit-event-type",
                "code": "rest"
            },
            "subtype": [{
                "system": "http://hl7.org/fhir/restful-interaction",
                "code": input.interaction
            }],
            "action": input.action,
            "recorded": recorded,
            "outcome": outcome,
            "agent": agents,
            "source": {
                "site": self.observer_display,
                "observer": { "display": self.observer_display }
            },
            "entity": entity,
        });

        if let Some(pid) = input.patient_id {
            event["patient"] = json!({ "reference": format!("Patient/{}", pid) });
        }

        if !contained.is_empty() {
            event["contained"] = json!(contained);
        }

        event
    }
}

fn build_standard_entity_details(input: &HttpAuditInput) -> JsonValue {
    build_standard_entity_details_opt(input).unwrap_or_else(|| json!([]))
}

fn build_standard_entity_details_opt(input: &HttpAuditInput) -> Option<JsonValue> {
    let mut details: Vec<JsonValue> = Vec::new();

    if let Some(request_id) = &input.request_id {
        details.push(json!({
            "type": { "text": "request-id" },
            "valueString": request_id
        }));
    }

    if let Some(ua) = &input.user_agent {
        details.push(json!({
            "type": { "text": "user-agent" },
            "valueString": ua
        }));
    }

    if details.is_empty() {
        None
    } else {
        Some(json!(details))
    }
}

#[derive(Debug, Clone)]
struct AuditLogInsert {
    event_type: String,
    action: String,
    http_method: String,
    fhir_action: String,
    resource_type: Option<String>,
    resource_id: Option<String>,
    version_id: Option<i32>,
    patient_id: Option<String>,
    client_id: Option<String>,
    user_id: Option<String>,
    scopes: Option<Vec<String>>,
    token_type: String,
    client_ip: Option<String>,
    user_agent: Option<String>,
    request_id: Option<String>,
    status_code: i32,
    outcome: String,
    audit_event: JsonValue,
    details: JsonValue,
}

fn infer_token_type(principal: Option<&Principal>) -> String {
    let Some(p) = principal else {
        return "anonymous".to_string();
    };
    match &p.client_id {
        Some(cid) if *cid == p.subject => "system".to_string(),
        Some(_) => "user".to_string(),
        None => "unknown".to_string(),
    }
}

fn build_details(input: &HttpAuditInput, principal: Option<&Principal>) -> JsonValue {
    let mut v = json!({
        "smart": {
            "client_id": principal.and_then(|p| p.client_id.clone()),
            "scopes": principal.map(|p| p.scopes.clone()).unwrap_or_default(),
        },
        "http": {
            "status": input.status,
        }
    });

    if let Some((rt, id)) = &input.target {
        v["target"] = json!({
            "resource_type": rt,
            "resource_id": id,
        });
    }
    if let Some(pid) = &input.patient_id {
        if !v.get("target").is_some_and(|t| t.is_object()) {
            v["target"] = json!({});
        }
        v["target"]["patient_id"] = json!(pid);
    }

    if let Some(h) = &input.query_harmonized {
        v["http"]["request"] = json!(h);
    }

    v
}

async fn insert_audit_log_row(pool: &PgPool, row: &AuditLogInsert) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO audit_log (
            event_type,
            action,
            http_method,
            fhir_action,
            resource_type,
            resource_id,
            version_id,
            patient_id,
            client_id,
            user_id,
            scopes,
            token_type,
            client_ip,
            user_agent,
            request_id,
            status_code,
            outcome,
            audit_event,
            details
        ) VALUES (
            $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19
        )
        "#,
    )
    .bind(&row.event_type)
    .bind(&row.action)
    .bind(&row.http_method)
    .bind(&row.fhir_action)
    .bind(&row.resource_type)
    .bind(&row.resource_id)
    .bind(row.version_id)
    .bind(&row.patient_id)
    .bind(&row.client_id)
    .bind(&row.user_id)
    .bind(&row.scopes)
    .bind(&row.token_type)
    .bind(&row.client_ip)
    .bind(&row.user_agent)
    .bind(&row.request_id)
    .bind(row.status_code)
    .bind(&row.outcome)
    .bind(&row.audit_event)
    .bind(&row.details)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_pool() -> PgPool {
        sqlx::PgPool::connect_lazy("postgres://localhost/does_not_exist").unwrap()
    }

    #[tokio::test]
    async fn builds_r5_audit_event_shape() {
        let config = crate::config::Config::load().unwrap();
        let cache = std::sync::Arc::new(crate::runtime_config::RuntimeConfigCache::new(
            std::sync::Arc::new(config),
        ));
        let svc = AuditService::new(
            cache,
            "R5".to_string(),
            "FHIR Server".to_string(),
            dummy_pool(),
        );

        let evt = svc.build_r5_http_event(HttpAuditInput {
            method: "GET".to_string(),
            interaction: "read".to_string(),
            action: "R".to_string(),
            status: 200,
            request_id: Some("req-1".to_string()),
            principal: Some(Principal {
                subject: "sub".to_string(),
                scopes: vec![],
                issuer: None,
                audience: None,
                client_id: None,
                patient: None,
            }),
            client_ip: Some("127.0.0.1".to_string()),
            user_agent: Some("ua".to_string()),
            target: Some(("Patient".to_string(), "123".to_string())),
            patient_id: Some("123".to_string()),
            query_base64: None,
            query_harmonized: None,
            operation_outcome: None,
        });

        assert_eq!(
            evt.get("resourceType").and_then(|v| v.as_str()),
            Some("AuditEvent")
        );
        assert!(evt.get("type").is_some());
        assert!(evt.get("agent").is_some());
        assert!(evt.get("source").is_some());
        assert!(evt.get("recorded").is_some());
        assert!(evt.get("outcome").is_some());
        assert!(evt.get("entity").is_some());
    }
}
