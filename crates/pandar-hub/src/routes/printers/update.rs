use axum::{
    Json, extract::Path, extract::State, extract::rejection::JsonRejection, http::HeaderMap,
};

use crate::{
    AppState,
    printer_events::printer_event_printer,
    repositories::UserRole,
    routes::{ApiError, auth},
};

use super::{PrinterResponse, UpdatePrinterRequest, helpers::parse_printer_id};

pub(in crate::routes) async fn update_printer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((tenant_id, printer_id)): Path<(String, String)>,
    payload: Result<Json<UpdatePrinterRequest>, JsonRejection>,
) -> Result<Json<PrinterResponse>, ApiError> {
    let tenant_id = super::super::parse_tenant_id(&tenant_id)?;
    let auth =
        auth::authorize_tenant_principal(&state, &headers, tenant_id, UserRole::Operator).await?;
    let printer_id = parse_printer_id(&printer_id)?;
    let Json(payload) = payload.map_err(|_| ApiError::bad_request("bad_request"))?;

    let Some(printer) = state
        .printers()
        .get_for_tenant(tenant_id, printer_id)
        .await?
    else {
        return Err(ApiError::not_found("printer_not_found"));
    };
    let (name, host, access_code) =
        payload.into_fields(printer.host.clone(), printer.access_code.clone())?;

    let updated = state
        .printers()
        .update_details_with_audit(
            tenant_id,
            printer_id,
            name,
            host,
            access_code,
            auth::audit_actor(&auth),
        )
        .await?;
    let materials = state
        .materials()
        .latest_for_printer(tenant_id, printer_id)
        .await?;

    Ok(Json(printer_event_printer(updated, materials)))
}
