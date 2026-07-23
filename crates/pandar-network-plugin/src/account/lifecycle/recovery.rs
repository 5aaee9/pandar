use crate::{
    cancellation::RequestCancellation, connection::no_auth_rotation::NoAuthRotationOutcome,
};

pub(super) fn retry_pending_revocation_with_cancellation(
    config_dir: &str,
    cancellation: RequestCancellation,
) -> NoAuthRotationOutcome {
    let response = super::take_http(super::super::revocation::revocation_result(
        super::super::revocation::revoke_all_with_cancellation(config_dir, cancellation),
    ));
    if response.status != 0 {
        eprintln!(
            "pandar pending plugin session revoke failed: status={} http_code={} body={}",
            response.status, response.http_code, response.body
        );
    }
    response
}
