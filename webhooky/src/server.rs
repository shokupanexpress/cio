use std::{collections::HashMap, env, fs::File, pin::Pin, sync::Arc};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use cio_api::{analytics::NewPageView, db::Database, functions::Function, swag_store::Order};
use clokwerk::{AsyncScheduler, TimeUnits};
use docusign::DocuSign;
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseAccepted,
    HttpResponseOk, HttpServerStarter, Path, Query, RequestContext, TypedBody, UntypedBody,
};
use google_drive::Client as GoogleDrive;
use gusto_api::Client as Gusto;
use http::StatusCode;
use log::{info, warn};
use mailchimp_api::MailChimp;
use quickbooks::QuickBooks;
use ramp_api::Client as Ramp;
use schemars::JsonSchema;
use sentry::{protocol, Hub};
use serde::{Deserialize, Serialize};
use shipbob::Client as ShipBob;
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    iterator::Signals,
};
use slack_chat_api::Slack;
use zoom_api::Client as Zoom;

use crate::github_types::GitHubWebhook;

pub async fn server(s: crate::Server, logger: slog::Logger, debug: bool) -> Result<()> {
    /*
     * We must specify a configuration with a bind address.  We'll use 127.0.0.1
     * since it's available and won't expose this server outside the host.  We
     * request port 8080.
     */
    let config_dropshot = ConfigDropshot {
        bind_address: s.address.parse()?,
        request_body_max_bytes: 107374182400, // 100 Gigiabytes.
        tls: None,
    };

    /*
     * For simplicity, we'll configure an "info"-level logger that writes to
     * stderr assuming that it's a terminal.
     */
    let mut log_level = ConfigLoggingLevel::Info;
    if debug {
        log_level = ConfigLoggingLevel::Debug;
    }
    let config_logging = ConfigLogging::StderrTerminal { level: log_level };
    let log = config_logging.to_logger("webhooky-server")?;

    // Describe the API.
    let mut api = ApiDescription::new();
    /*
     * Register our endpoint and its handler function.  The "endpoint" macro
     * specifies the HTTP method and URI path that identify the endpoint,
     * allowing this metadata to live right alongside the handler function.
     */
    api.register(ping).unwrap();
    api.register(github_rate_limit).unwrap();
    api.register(listen_airtable_applicants_request_background_check_webhooks)
        .unwrap();
    api.register(listen_airtable_applicants_update_webhooks).unwrap();
    api.register(listen_airtable_assets_items_print_barcode_label_webhooks)
        .unwrap();
    api.register(listen_airtable_employees_print_home_address_label_webhooks)
        .unwrap();
    api.register(listen_airtable_certificates_renew_webhooks).unwrap();
    api.register(listen_airtable_shipments_inbound_create_webhooks).unwrap();
    api.register(listen_airtable_shipments_outbound_create_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_outbound_reprint_label_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_outbound_reprint_receipt_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_outbound_resend_shipment_status_email_to_recipient_webhooks)
        .unwrap();
    api.register(listen_airtable_shipments_outbound_schedule_pickup_webhooks)
        .unwrap();
    api.register(listen_airtable_swag_inventory_items_print_barcode_labels_webhooks)
        .unwrap();
    api.register(listen_analytics_page_view_webhooks).unwrap();
    api.register(listen_application_submit_requests).unwrap();
    api.register(listen_applicant_review_requests).unwrap();
    api.register(listen_application_files_upload_requests).unwrap();
    api.register(listen_auth_docusign_callback).unwrap();
    api.register(listen_auth_docusign_consent).unwrap();
    api.register(listen_auth_github_callback).unwrap();
    api.register(listen_auth_github_consent).unwrap();
    api.register(listen_auth_google_callback).unwrap();
    api.register(listen_auth_google_consent).unwrap();
    api.register(listen_auth_gusto_callback).unwrap();
    api.register(listen_auth_gusto_consent).unwrap();
    api.register(listen_auth_mailchimp_callback).unwrap();
    api.register(listen_auth_mailchimp_consent).unwrap();
    api.register(listen_auth_plaid_callback).unwrap();
    api.register(listen_auth_ramp_callback).unwrap();
    api.register(listen_auth_ramp_consent).unwrap();
    api.register(listen_auth_shipbob_callback).unwrap();
    api.register(listen_auth_shipbob_consent).unwrap();
    api.register(listen_auth_zoom_callback).unwrap();
    api.register(listen_auth_zoom_consent).unwrap();
    api.register(listen_auth_zoom_deauthorization).unwrap();
    api.register(listen_auth_slack_callback).unwrap();
    api.register(listen_auth_slack_consent).unwrap();
    api.register(listen_auth_quickbooks_callback).unwrap();
    api.register(listen_auth_quickbooks_consent).unwrap();
    api.register(listen_checkr_background_update_webhooks).unwrap();
    api.register(listen_docusign_envelope_update_webhooks).unwrap();
    api.register(listen_emails_incoming_sendgrid_parse_webhooks).unwrap();
    api.register(listen_github_webhooks).unwrap();
    api.register(listen_mailchimp_mailing_list_webhooks).unwrap();
    api.register(listen_mailchimp_rack_line_webhooks).unwrap();
    api.register(listen_products_sold_count_requests).unwrap();
    api.register(listen_shippo_tracking_update_webhooks).unwrap();
    api.register(listen_easypost_tracking_update_webhooks).unwrap();
    api.register(listen_slack_commands_webhooks).unwrap();
    api.register(listen_slack_interactive_webhooks).unwrap();
    api.register(listen_shipbob_webhooks).unwrap();
    api.register(listen_store_order_create).unwrap();
    api.register(ping_mailchimp_mailing_list_webhooks).unwrap();
    api.register(ping_mailchimp_rack_line_webhooks).unwrap();
    api.register(trigger_rfd_update_by_number).unwrap();
    api.register(trigger_cleanup_create).unwrap();

    api.register(trigger_sync_analytics_create).unwrap();
    api.register(trigger_sync_api_tokens_create).unwrap();
    api.register(trigger_sync_applications_create).unwrap();
    api.register(trigger_sync_asset_inventory_create).unwrap();
    api.register(trigger_sync_companies_create).unwrap();
    api.register(trigger_sync_configs_create).unwrap();
    api.register(trigger_sync_finance_create).unwrap();
    api.register(trigger_sync_functions_create).unwrap();
    api.register(trigger_sync_huddles_create).unwrap();
    api.register(trigger_sync_interviews_create).unwrap();
    api.register(trigger_sync_journal_clubs_create).unwrap();
    api.register(trigger_sync_mailing_lists_create).unwrap();
    api.register(trigger_sync_other_create).unwrap();
    api.register(trigger_sync_recorded_meetings_create).unwrap();
    api.register(trigger_sync_repos_create).unwrap();
    api.register(trigger_sync_rfds_create).unwrap();
    api.register(trigger_sync_shipments_create).unwrap();
    api.register(trigger_sync_shorturls_create).unwrap();
    api.register(trigger_sync_swag_inventory_create).unwrap();
    api.register(trigger_sync_travel_create).unwrap();

    api.register(listen_get_function_by_uuid).unwrap();
    api.register(listen_get_function_logs_by_uuid).unwrap();
    api.register(api_get_schema).unwrap();

    // Create the API schema.
    let mut api_definition = &mut api.openapi(&"Webhooks API", &clap::crate_version!());
    api_definition = api_definition
        .description("Internal webhooks server for listening to several third party webhooks")
        .contact_url("https://oxide.computer")
        .contact_email("webhooks@oxide.computer");
    let schema = api_definition.json()?;

    if let Some(spec_file) = &s.spec_file {
        info!("writing OpenAPI spec to {}...", spec_file.to_str().unwrap());
        let mut buffer = File::create(spec_file)?;
        api_definition.write(&mut buffer)?;
    }

    /*
     * The functions that implement our API endpoints will share this context.
     */
    let api_context = Context::new(schema, logger).await;

    // Copy the Server struct so we can move it into our loop.
    if s.do_cron {
        /*
         * Setup our cron jobs, with our timezone.
         */
        let mut scheduler = AsyncScheduler::with_tz(chrono_tz::US::Pacific);
        // TODO: probably a better way to do this without all the freaking clones.
        scheduler
            .every(1.day())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-analytics")});
        scheduler
            .every(23.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-api-tokens")});
        scheduler
            .every(6.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-applications")});
        scheduler
            .every(2.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-asset-inventory")});
        scheduler
            .every(12.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-companies")});
        scheduler
            .every(4.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-configs")});
        scheduler
            .every(6.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-finance")});
        scheduler
            .every(12.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-functions")});
        scheduler
            .every(1.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-huddles")});
        scheduler
            .every(4.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-interviews")});
        scheduler
            .every(12.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-journal-clubs")});
        scheduler
            .every(20.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-mailing-lists")});
        scheduler
            .every(18.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-other")});
        scheduler
            .every(2.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-recorded-meetings")});
        scheduler
            .every(16.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-repos")});
        scheduler
            .every(14.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-rfds")});
        scheduler
            .every(2.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-shipments")});
        scheduler
            .every(3.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-shorturls")});
        scheduler
            .every(9.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-swag-inventory")});
        scheduler
            .every(5.hours())
            .run(enclose! { (api_context) move || api_context.create_do_job_fn("sync-travel")});
        // TODO: Run the RFD changelog.
        // Turn this on once we know cron jobs are consistently running.
        /* scheduler.every(Monday).at("8:00 am").run(enclose!{ (api_context) move || api_context.create_do_job_fn("send-rfd-changelog")}); */

        tokio::spawn(async move {
            info!("starting cron job scheduler...");

            loop {
                scheduler.run_pending().await;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });
    }

    /*
     * Set up the server.
     */
    let server = HttpServerStarter::new(&config_dropshot, api, api_context.clone(), &log)
        .map_err(|error| anyhow!("failed to create server: {}", error))?
        .start();

    // For Cloud run & ctrl+c, shutdown gracefully.
    // "The main process inside the container will receive SIGTERM, and after a grace period,
    // SIGKILL."
    // Regsitering SIGKILL here will panic at runtime, so let's avoid that.
    let mut signals = Signals::new(&[SIGINT, SIGTERM])?;

    tokio::spawn(enclose! { (api_context) async move {
        for sig in signals.forever() {
            info!("received signal: {:?}", sig);
            info!("triggering cleanup...");

            // Run the cleanup job.
            if let Err(e) = do_cleanup(&api_context).await {
                sentry::integrations::anyhow::capture_anyhow(&e);
            }
            // Exit the process.
            info!("all clean, exiting!");
            std::process::exit(0);
        }
    }});

    server.await.unwrap();

    Ok(())
}

/**
 * Application-specific context (state shared by handler functions)
 */
#[derive(Clone, Debug)]
pub struct Context {
    pub db: Database,

    pub sec: Arc<steno::SecClient>,

    pub schema: serde_json::Value,
}

impl Context {
    /**
     * Return a new Context.
     */
    #[tracing::instrument]
    pub async fn new(schema: serde_json::Value, logger: slog::Logger) -> Context {
        let db = Database::new().await;

        let sec = steno::sec(logger, Arc::new(db.clone()));

        // Create the context.
        Context {
            db,
            sec: Arc::new(sec),
            schema,
        }
    }

    #[tracing::instrument]
    pub fn create_do_job_fn(&self, job: &str) -> Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
        Box::pin(do_job(self.clone(), job.to_string()))
    }
}

#[tracing::instrument]
pub async fn do_job(ctx: Context, job: String) {
    info!("triggering cron job `{}`", job);

    sentry::start_session();
    match crate::handlers_cron::handle_reexec_cmd(&ctx, &job, true).await {
        Ok(_) => {
            sentry::end_session();
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            handle_anyhow_err_as_http_err(e);
        }
    }
}

/*
 * HTTP API interface
 */

/**
 * Return the OpenAPI schema in JSON format.
 */
#[endpoint {
    method = GET,
    path = "/",
}]
#[tracing::instrument]
async fn api_get_schema(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<serde_json::Value>, HttpError> {
    let mut txn = start_sentry_http_transaction(rqctx.clone()).await;
    let api_context = rqctx.context();

    txn.finish(http::StatusCode::OK);
    Ok(HttpResponseOk(api_context.schema.clone()))
}

/** Return pong. */
#[endpoint {
    method = GET,
    path = "/ping",
}]
#[tracing::instrument]
async fn ping(_rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<String>, HttpError> {
    Ok(HttpResponseOk("pong".to_string()))
}

#[derive(Deserialize, Serialize, Default, Clone, Debug, JsonSchema)]
pub struct CounterResponse {
    #[serde(default)]
    pub count: i32,
}

/** Return the count of products sold. */
#[endpoint {
    method = GET,
    path = "/products/sold/count",
}]
#[tracing::instrument]
async fn listen_products_sold_count_requests(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<CounterResponse>, HttpError> {
    sentry::start_session();
    let mut txn = start_sentry_http_transaction(rqctx.clone()).await;

    match crate::handlers::handle_products_sold_count(rqctx).await {
        Ok(r) => {
            txn.finish(http::StatusCode::OK);
            sentry::end_session();
            Ok(HttpResponseOk(r))
        }
        // Send the error to sentry.
        Err(e) => {
            txn.finish(http::StatusCode::INTERNAL_SERVER_ERROR);
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for GitHub webhooks. */
#[endpoint {
    method = POST,
    path = "/github",
}]
#[tracing::instrument]
async fn listen_github_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<GitHubWebhook>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers_github::handle_github(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[derive(Deserialize, Debug, JsonSchema)]
pub struct RFDPathParams {
    pub num: i32,
}

/** Trigger an update for an RFD. */
#[endpoint {
    method = POST,
    path = "/rfd/{num}",
}]
#[tracing::instrument]
async fn trigger_rfd_update_by_number(
    rqctx: Arc<RequestContext<Context>>,
    path_params: Path<RFDPathParams>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_rfd_update_by_number(rqctx, path_params).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get our current GitHub rate limit. */
#[endpoint {
    method = GET,
    path = "/github/ratelimit",
}]
#[tracing::instrument]
async fn github_rate_limit(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseOk<GitHubRateLimit>, HttpError> {
    sentry::start_session();

    match crate::handlers::handle_github_rate_limit(rqctx).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseOk(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/// A GitHub RateLimit
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct GitHubRateLimit {
    #[serde(default)]
    pub limit: u32,
    #[serde(default)]
    pub remaining: u32,
    #[serde(default)]
    pub reset: String,
}

/**
 * Listen for a button pressed to print a home address label for employees.
 */
#[endpoint {
    method = POST,
    path = "/airtable/employees/print_home_address_label",
}]
#[tracing::instrument]
async fn listen_airtable_employees_print_home_address_label_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_employees_print_home_address_label(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to renew a certificate.
 */
#[endpoint {
    method = POST,
    path = "/airtable/certificates/renew",
}]
#[tracing::instrument]
async fn listen_airtable_certificates_renew_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_certificates_renew(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to print a barcode label for an asset item.
 */
#[endpoint {
    method = POST,
    path = "/airtable/assets/items/print_barcode_label",
}]
#[tracing::instrument]
async fn listen_airtable_assets_items_print_barcode_label_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_assets_items_print_barcode_label(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to print barcode labels for a swag inventory item.
 */
#[endpoint {
    method = POST,
    path = "/airtable/swag/inventory/items/print_barcode_labels",
}]
#[tracing::instrument]
async fn listen_airtable_swag_inventory_items_print_barcode_labels_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_swag_inventory_items_print_barcode_labels(rqctx, body_param).await
    {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to request a background check for an applicant.
 */
#[endpoint {
    method = POST,
    path = "/airtable/applicants/request_background_check",
}]
#[tracing::instrument]
async fn listen_airtable_applicants_request_background_check_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_applicants_request_background_check(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for rows updated in our Airtable workspace.
 * These are set up with an Airtable script on the workspaces themselves.
 */
#[endpoint {
    method = POST,
    path = "/airtable/applicants/update",
}]
#[tracing::instrument]
async fn listen_airtable_applicants_update_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_applicants_update(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for rows created in our Airtable workspace.
 * These are set up with an Airtable script on the workspaces themselves.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/create",
}]
#[tracing::instrument]
async fn listen_airtable_shipments_outbound_create_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_shipments_outbound_create(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/// An Airtable row event.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct AirtableRowEvent {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub record_id: String,
    #[serde(default)]
    pub cio_company_id: i32,
}

/**
 * Listen for a button pressed to reprint a label for an outbound shipment.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/reprint_label",
}]
#[tracing::instrument]
async fn listen_airtable_shipments_outbound_reprint_label_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_shipments_outbound_reprint_label(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to reprint a receipt for an outbound shipment.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/reprint_receipt",
}]
#[tracing::instrument]
async fn listen_airtable_shipments_outbound_reprint_receipt_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_shipments_outbound_reprint_receipt(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to resend a shipment status email to the recipient for an outbound shipment.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/resend_shipment_status_email_to_recipient",
}]
#[tracing::instrument]
async fn listen_airtable_shipments_outbound_resend_shipment_status_email_to_recipient_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) =
        crate::handlers::handle_airtable_shipments_outbound_resend_shipment_status_email_to_recipient(rqctx, body_param)
            .await
    {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for a button pressed to schedule a pickup for an outbound shipment.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/outbound/schedule_pickup",
}]
#[tracing::instrument]
async fn listen_airtable_shipments_outbound_schedule_pickup_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_shipments_outbound_schedule_pickup(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/// A SendGrid incoming email event.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct IncomingEmail {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub headers: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dkim: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "content-ids")]
    pub content_ids: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub to: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cc: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub html: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub from: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sender_ip: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub spam_report: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub envelope: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub attachments: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subject: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub spam_score: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "attachment-info")]
    pub attachment_info: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub charsets: String,
    #[serde(default, skip_serializing_if = "String::is_empty", alias = "SPF")]
    pub spf: String,
}

/**
 * Listen for emails coming inbound from SendGrid's parse API.
 * We use this for scanning for packages in emails.
 */
#[endpoint {
    method = POST,
    path = "/emails/incoming/sendgrid/parse",
}]
#[tracing::instrument]
async fn listen_emails_incoming_sendgrid_parse_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_emails_incoming_sendgrid_parse(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for applicant reviews being submitted for job applicants */
#[endpoint {
    method = POST,
    path = "/applicant/review/submit",
}]
#[tracing::instrument]
async fn listen_applicant_review_requests(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<cio_api::applicant_reviews::NewApplicantReview>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_applicant_review(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for applications being submitted for incoming job applications */
#[endpoint {
    method = POST,
    path = "/application/submit",
}]
#[tracing::instrument]
async fn listen_application_submit_requests(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<cio_api::application_form::ApplicationForm>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_application_submit(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/// Application file upload data.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct ApplicationFileUploadData {
    #[serde(default)]
    pub cio_company_id: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resume: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub materials: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub portfolio_pdf_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resume_contents: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub materials_contents: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub portfolio_pdf_contents: String,
}

/**
 * Listen for files being uploaded for incoming job applications */
#[endpoint {
    method = POST,
    path = "/application/files/upload",
}]
#[tracing::instrument]
async fn listen_application_files_upload_requests(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<ApplicationFileUploadData>,
) -> Result<HttpResponseOk<HashMap<String, String>>, HttpError> {
    sentry::start_session();

    match crate::handlers::handle_application_files_upload(rqctx, body_param).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseOk(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/**
 * Listen for rows created in our Airtable workspace.
 * These are set up with an Airtable script on the workspaces themselves.
 */
#[endpoint {
    method = POST,
    path = "/airtable/shipments/inbound/create",
}]
#[tracing::instrument]
async fn listen_airtable_shipments_inbound_create_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<AirtableRowEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_airtable_shipments_inbound_create(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for orders being created by the Oxide store.
 */
#[endpoint {
    method = POST,
    path = "/store/order",
}]
#[tracing::instrument]
async fn listen_store_order_create(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<Order>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_store_order_create(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/**
 * Listen for shipment tracking updated from EasyPost.
 */
#[endpoint {
    method = POST,
    path = "/easypost/tracking/update",
}]
#[tracing::instrument]
async fn listen_easypost_tracking_update_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<EasyPostTrackingUpdateEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_easypost_tracking_update(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/// An EasyPost tracking update event.
/// FROM: https://www.easypost.com/docs/api#events
#[derive(Debug, Clone, JsonSchema, Deserialize, Serialize)]
pub struct EasyPostTrackingUpdateEvent {
    /// "Event".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object: String,
    /// Unique identifier, begins with "evt_".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// "test" or "production"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mode: String,
    /// Result type and event name, see the "Possible Event Types" section for more information.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /* /// Previous values of relevant result attributes.
    #[serde(default)]
    pub previous_attributes: serde_json::Value,
    /// The object associated with the Event. See the "object" attribute on the result to determine
    /// its specific type. This field will not be returned when retrieving events directly from the
    /// API.
    #[serde(default)]
    pub result: serde_json::Value,*/
    /// The current status of the event. Possible values are "completed", "failed", "in_queue",
    /// "retrying", or "pending" (deprecated).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// Webhook URLs that have not yet been successfully notified as of the time this webhook event
    /// was sent. The URL receiving the Event will still be listed in pending_urls, as will any
    /// other URLs that receive the Event at the same time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_urls: Vec<String>,
    /// Webhook URLs that have already been successfully notified as of the time this webhook was
    /// sent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

/**
 * Listen for shipment tracking updated from Shippo.
 */
#[endpoint {
    method = POST,
    path = "/shippo/tracking/update",
}]
#[tracing::instrument]
async fn listen_shippo_tracking_update_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_shippo_tracking_update(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/// A Shippo tracking update event.
#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct ShippoTrackingUpdateEvent {
    #[serde(default)]
    pub data: shippo::TrackingStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub event: String,
    #[serde(default)]
    pub test: bool,
}

/** Listen for updates to our checkr background checks. */
#[endpoint {
    method = POST,
    path = "/checkr/background/update",
}]
#[tracing::instrument]
async fn listen_checkr_background_update_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<checkr::WebhookEvent>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_checkr_background_update(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct UserConsentURL {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

#[derive(Debug, Clone, Default, JsonSchema, Deserialize, Serialize)]
pub struct AuthCallback {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code: String,
    /// The state that we had passed in through the user consent URL.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(default, skip_serializing_if = "String::is_empty", rename = "realmId")]
    pub realm_id: String,
}

/** Get the consent URL for Google auth. */
#[endpoint {
    method = GET,
    path = "/auth/google/consent",
}]
#[tracing::instrument]
async fn listen_auth_google_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the Google client.
    // You can use any of the libs here, they all use the same endpoint
    // for tokens and we will send all the scopes.
    let g = GoogleDrive::new_from_env("", "").await;

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(&cio_api::companies::get_google_scopes()),
    }))
}

/** Listen for callbacks to Google auth. */
#[endpoint {
    method = GET,
    path = "/auth/google/callback",
}]
#[tracing::instrument]
async fn listen_auth_google_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers_auth::handle_auth_google_callback(rqctx, query_args).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for shipbob auth. */
#[endpoint {
    method = GET,
    path = "/auth/shipbob/consent",
}]
#[tracing::instrument]
async fn listen_auth_shipbob_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the shipbob client.
    // You can use any of the libs here, they all use the same endpoint
    // for tokens and we will send all the scopes.
    let g = ShipBob::new_from_env("", "", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        // We want to get the response as a form.
        url: g.user_consent_url(&cio_api::companies::get_shipbob_scopes()) + "&response_mode=form_post",
    }))
}

/** Listen for callbacks to shipbob auth. */
#[endpoint {
    method = POST,
    path = "/auth/shipbob/callback",
}]
#[tracing::instrument]
async fn listen_auth_shipbob_callback(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers_auth::handle_auth_shipbob_callback(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for GitHub auth. */
#[endpoint {
    method = GET,
    path = "/auth/github/consent",
}]
#[tracing::instrument]
async fn listen_auth_github_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: "https://github.com/apps/oxidecomputerbot/installations/new".to_string(),
    }))
}

/** Listen for callbacks to GitHub auth. */
#[endpoint {
    method = GET,
    path = "/auth/github/callback",
}]
#[tracing::instrument]
async fn listen_auth_github_callback(
    _rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = body_param.into_inner();

    warn!("github callback: {:?}", event);

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for MailChimp auth. */
#[endpoint {
    method = GET,
    path = "/auth/mailchimp/consent",
}]
#[tracing::instrument]
async fn listen_auth_mailchimp_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the MailChimp client.
    let g = MailChimp::new_from_env("", "", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(),
    }))
}

/** Listen for callbacks to MailChimp auth. */
#[endpoint {
    method = GET,
    path = "/auth/mailchimp/callback",
}]
#[tracing::instrument]
async fn listen_auth_mailchimp_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers_auth::handle_auth_mailchimp_callback(rqctx, query_args).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for Gusto auth. */
#[endpoint {
    method = GET,
    path = "/auth/gusto/consent",
}]
#[tracing::instrument]
async fn listen_auth_gusto_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the Gusto client.
    let g = Gusto::new_from_env("", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        // We don't need to define scopes for Gusto.
        url: g.user_consent_url(&[]),
    }))
}

/** Listen for callbacks to Gusto auth. */
#[endpoint {
    method = GET,
    path = "/auth/gusto/callback",
}]
#[tracing::instrument]
async fn listen_auth_gusto_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers_auth::handle_auth_gusto_callback(rqctx, query_args).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen to deauthorization requests for our Zoom app. */
#[endpoint {
    method = GET,
    path = "/auth/zoom/deauthorization",
}]
#[tracing::instrument]
async fn listen_auth_zoom_deauthorization(
    _rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    let event = body_param.into_inner();

    warn!("zoom deauthorization: {:?}", event);

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for Zoom auth. */
#[endpoint {
    method = GET,
    path = "/auth/zoom/consent",
}]
#[tracing::instrument]
async fn listen_auth_zoom_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the Zoom client.
    let g = Zoom::new_from_env("", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(&[]),
    }))
}

/** Listen for callbacks to Zoom auth. */
#[endpoint {
    method = GET,
    path = "/auth/zoom/callback",
}]
#[tracing::instrument]
async fn listen_auth_zoom_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers_auth::handle_auth_zoom_callback(rqctx, query_args).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for Ramp auth. */
#[endpoint {
    method = GET,
    path = "/auth/ramp/consent",
}]
#[tracing::instrument]
async fn listen_auth_ramp_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the Ramp client.
    let g = Ramp::new_from_env("", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(&[
            "transactions:read".to_string(),
            "users:read".to_string(),
            "users:write".to_string(),
            "receipts:read".to_string(),
            "cards:read".to_string(),
            "departments:read".to_string(),
            "reimbursements:read".to_string(),
        ]),
    }))
}

/** Listen for callbacks to Ramp auth. */
#[endpoint {
    method = GET,
    path = "/auth/ramp/callback",
}]
#[tracing::instrument]
async fn listen_auth_ramp_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers_auth::handle_auth_ramp_callback(rqctx, query_args).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for Slack auth. */
#[endpoint {
    method = GET,
    path = "/auth/slack/consent",
}]
#[tracing::instrument]
async fn listen_auth_slack_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the Slack client.
    let s = Slack::new_from_env("", "", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: s.user_consent_url(),
    }))
}

/** Listen for callbacks to Slack auth. */
#[endpoint {
    method = GET,
    path = "/auth/slack/callback",
}]
#[tracing::instrument]
async fn listen_auth_slack_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers_auth::handle_auth_slack_callback(rqctx, query_args).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for QuickBooks auth. */
#[endpoint {
    method = GET,
    path = "/auth/quickbooks/consent",
}]
#[tracing::instrument]
async fn listen_auth_quickbooks_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the QuickBooks client.
    let g = QuickBooks::new_from_env("", "", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(),
    }))
}

/** Listen for callbacks to QuickBooks auth. */
#[endpoint {
    method = GET,
    path = "/auth/quickbooks/callback",
}]
#[tracing::instrument]
async fn listen_auth_quickbooks_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers_auth::handle_auth_quickbooks_callback(rqctx, query_args).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen for webhooks from Plaid. */
#[endpoint {
    method = POST,
    path = "/plaid",
}]
#[tracing::instrument]
async fn listen_auth_plaid_callback(
    _rqctx: Arc<RequestContext<Context>>,
    body_args: TypedBody<serde_json::Value>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();
    let event = body_args.into_inner();

    warn!("plaid callback: {:?}", event);

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Get the consent URL for DocuSign auth. */
#[endpoint {
    method = GET,
    path = "/auth/docusign/consent",
}]
#[tracing::instrument]
async fn listen_auth_docusign_consent(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<UserConsentURL>, HttpError> {
    sentry::start_session();

    // Initialize the DocuSign client.
    let g = DocuSign::new_from_env("", "", "", "");

    sentry::end_session();
    Ok(HttpResponseOk(UserConsentURL {
        url: g.user_consent_url(),
    }))
}

/** Listen for callbacks to DocuSign auth. */
#[endpoint {
    method = GET,
    path = "/auth/docusign/callback",
}]
#[tracing::instrument]
async fn listen_auth_docusign_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers_auth::handle_auth_docusign_callback(rqctx, query_args).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen for updates to our docusign envelopes. */
#[endpoint {
    method = POST,
    path = "/docusign/envelope/update",
}]
#[tracing::instrument]
async fn listen_docusign_envelope_update_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<docusign::Envelope>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_docusign_envelope_update(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen for analytics page view events. */
#[endpoint {
    method = POST,
    path = "/analytics/page_view",
}]
#[tracing::instrument]
async fn listen_analytics_page_view_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<NewPageView>,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_analytics_page_view(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Ping endpoint for MailChimp mailing list webhooks. */
#[endpoint {
    method = GET,
    path = "/mailchimp/mailing_list",
}]
#[tracing::instrument]
async fn ping_mailchimp_mailing_list_webhooks(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<String>, HttpError> {
    Ok(HttpResponseOk("ok".to_string()))
}

/** Listen for MailChimp mailing list webhooks. */
#[endpoint {
    method = POST,
    path = "/mailchimp/mailing_list",
}]
#[tracing::instrument]
async fn listen_mailchimp_mailing_list_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_mailchimp_mailing_list(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Ping endpoint for MailChimp rack line webhooks. */
#[endpoint {
    method = GET,
    path = "/mailchimp/rack_line",
}]
#[tracing::instrument]
async fn ping_mailchimp_rack_line_webhooks(
    _rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseOk<String>, HttpError> {
    Ok(HttpResponseOk("ok".to_string()))
}

/** Listen for MailChimp rack line webhooks. */
#[endpoint {
    method = POST,
    path = "/mailchimp/rack_line",
}]
#[tracing::instrument]
async fn listen_mailchimp_rack_line_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<HttpResponseAccepted<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_mailchimp_rack_line(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseAccepted("ok".to_string()))
}

/** Listen for Slack commands webhooks. */
#[endpoint {
    method = POST,
    path = "/slack/commands",
}]
#[tracing::instrument]
async fn listen_slack_commands_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<HttpResponseOk<serde_json::Value>, HttpError> {
    sentry::start_session();

    match crate::handlers::handle_slack_commands(rqctx, body_param).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseOk(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for Slack interactive webhooks. */
#[endpoint {
    method = POST,
    path = "/slack/interactive",
}]
#[tracing::instrument]
async fn listen_slack_interactive_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: UntypedBody,
) -> Result<HttpResponseOk<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_slack_interactive(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseOk("ok".to_string()))
}

/** Listen for shipbob webhooks. */
#[endpoint {
    method = POST,
    path = "/shipbob",
}]
#[tracing::instrument]
async fn listen_shipbob_webhooks(
    rqctx: Arc<RequestContext<Context>>,
    body_param: TypedBody<serde_json::Value>,
) -> Result<HttpResponseOk<String>, HttpError> {
    sentry::start_session();

    if let Err(e) = crate::handlers::handle_shipbob(rqctx, body_param).await {
        // Send the error to sentry.
        return Err(handle_anyhow_err_as_http_err(e));
    }

    sentry::end_session();
    Ok(HttpResponseOk("ok".to_string()))
}

#[derive(Deserialize, Debug, JsonSchema)]
pub struct FunctionPathParams {
    pub uuid: String,
}

/** Get information about a function by its uuid. */
#[endpoint {
    method = GET,
    path = "/functions/{uuid}",
}]
#[tracing::instrument]
async fn listen_get_function_by_uuid(
    rqctx: Arc<RequestContext<Context>>,
    path_params: Path<FunctionPathParams>,
) -> Result<HttpResponseOk<Function>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_get_function_by_uuid(rqctx, path_params).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseOk(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Get a functions logs by its uuid. */
#[endpoint {
    method = GET,
    path = "/functions/{uuid}/logs",
}]
#[tracing::instrument]
async fn listen_get_function_logs_by_uuid(
    rqctx: Arc<RequestContext<Context>>,
    path_params: Path<FunctionPathParams>,
) -> Result<HttpResponseOk<String>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_get_function_logs_by_uuid(rqctx, path_params).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseOk(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync repos. */
#[endpoint {
    method = POST,
    path = "/run/sync-repos",
}]
#[tracing::instrument]
async fn trigger_sync_repos_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-repos", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync RFDs. */
#[endpoint {
    method = POST,
    path = "/run/sync-rfds",
}]
#[tracing::instrument]
async fn trigger_sync_rfds_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-rfds", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync travel. */
#[endpoint {
    method = POST,
    path = "/run/sync-travel",
}]
#[tracing::instrument]
async fn trigger_sync_travel_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-travel", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync functions. */
#[endpoint {
    method = POST,
    path = "/run/sync-functions",
}]
#[tracing::instrument]
async fn trigger_sync_functions_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-functions", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync finance. */
#[endpoint {
    method = POST,
    path = "/run/sync-finance",
}]
#[tracing::instrument]
async fn trigger_sync_finance_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-finance", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync shipments. */
#[endpoint {
    method = POST,
    path = "/run/sync-shipments",
}]
#[tracing::instrument]
async fn trigger_sync_shipments_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-shipments", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync shorturls. */
#[endpoint {
    method = POST,
    path = "/run/sync-shorturls",
}]
#[tracing::instrument]
async fn trigger_sync_shorturls_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-shorturls", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync configs. */
#[endpoint {
    method = POST,
    path = "/run/sync-configs",
}]
#[tracing::instrument]
async fn trigger_sync_configs_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-configs", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync recorded meetings. */
#[endpoint {
    method = POST,
    path = "/run/sync-recorded-meetings",
}]
#[tracing::instrument]
async fn trigger_sync_recorded_meetings_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-recorded-meetings", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync asset inventory. */
#[endpoint {
    method = POST,
    path = "/run/sync-asset-inventory",
}]
#[tracing::instrument]
async fn trigger_sync_asset_inventory_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-asset-inventory", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync swag inventory. */
#[endpoint {
    method = POST,
    path = "/run/sync-swag-inventory",
}]
#[tracing::instrument]
async fn trigger_sync_swag_inventory_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-swag-inventory", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync interviews. */
#[endpoint {
    method = POST,
    path = "/run/sync-interviews",
}]
#[tracing::instrument]
async fn trigger_sync_interviews_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-interviews", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync applications. */
#[endpoint {
    method = POST,
    path = "/run/sync-applications",
}]
#[tracing::instrument]
async fn trigger_sync_applications_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-applications", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync analytics. */
#[endpoint {
    method = POST,
    path = "/run/sync-analytics",
}]
#[tracing::instrument]
async fn trigger_sync_analytics_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-analytics", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync companies. */
#[endpoint {
    method = POST,
    path = "/run/sync-companies",
}]
#[tracing::instrument]
async fn trigger_sync_companies_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-companies", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync other. */
#[endpoint {
    method = POST,
    path = "/run/sync-other",
}]
#[tracing::instrument]
async fn trigger_sync_other_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-other", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync huddles. */
#[endpoint {
    method = POST,
    path = "/run/sync-huddles",
}]
#[tracing::instrument]
async fn trigger_sync_huddles_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-huddles", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync mailing lists. */
#[endpoint {
    method = POST,
    path = "/run/sync-mailing-lists",
}]
#[tracing::instrument]
async fn trigger_sync_mailing_lists_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-mailing-lists", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync journal clubs. */
#[endpoint {
    method = POST,
    path = "/run/sync-journal-clubs",
}]
#[tracing::instrument]
async fn trigger_sync_journal_clubs_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-journal-clubs", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a function run of sync api tokens. */
#[endpoint {
    method = POST,
    path = "/run/sync-api-tokens",
}]
#[tracing::instrument]
async fn trigger_sync_api_tokens_create(
    rqctx: Arc<RequestContext<Context>>,
) -> Result<HttpResponseAccepted<uuid::Uuid>, HttpError> {
    sentry::start_session();

    match crate::handlers_cron::handle_reexec_cmd(rqctx.context(), "sync-api-tokens", true).await {
        Ok(r) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(r))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

/** Listen for triggering a cleanup of all in-progress sagas, we typically run this when the server
 * is shutting down. */
#[endpoint {
    method = POST,
    path = "/run/cleanup",
}]
#[tracing::instrument]
async fn trigger_cleanup_create(rqctx: Arc<RequestContext<Context>>) -> Result<HttpResponseAccepted<()>, HttpError> {
    sentry::start_session();

    let ctx = rqctx.context();

    match do_cleanup(ctx).await {
        Ok(_) => {
            sentry::end_session();
            Ok(HttpResponseAccepted(()))
        }
        // Send the error to sentry.
        Err(e) => {
            sentry::end_session();
            Err(handle_anyhow_err_as_http_err(e))
        }
    }
}

#[tracing::instrument]
async fn do_cleanup(ctx: &Context) -> Result<()> {
    let sec = &ctx.sec;
    // Get all our sagas.
    let sagas = sec.saga_list(None, std::num::NonZeroU32::new(1000).unwrap()).await;

    // TODO: Shutdown the executer.
    // This causes a compile time error, figure it out.
    //sec.shutdown().await;

    // Set all the in-progress sagas to 'cancelled'.
    for saga in sagas {
        // Get the saga in the database.
        if let Some(mut f) = Function::get_from_db(&ctx.db, saga.id.to_string()).await {
            // We only care about jobs that aren't completed.
            if f.status != octorust::types::JobStatus::Completed.to_string() {
                // Let's set the job to "Completed".
                f.status = octorust::types::JobStatus::Completed.to_string();
                // Let's set the conclusion to "Cancelled".
                f.conclusion = octorust::types::Conclusion::Cancelled.to_string();
                f.completed_at = Some(Utc::now());

                f.update(&ctx.db).await.map_err(handle_anyhow_err_as_http_err)?;
                info!("set saga `{}` `{}` as `{}`", f.name, f.saga_id, f.status);
            }
        }
    }

    Ok(())
}

#[tracing::instrument]
fn handle_anyhow_err_as_http_err(err: anyhow::Error) -> HttpError {
    // Send to sentry.
    sentry::integrations::anyhow::capture_anyhow(&anyhow::anyhow!("{:?}", err));
    sentry::end_session();

    // We use the debug formatting here so we get the stack trace.
    return HttpError::for_internal_error(format!("{:?}", err));
}

#[derive(Debug, Clone)]
pub struct SentryTransaction {
    transaction: sentry::TransactionOrSpan,
    parent_span: Option<sentry::TransactionOrSpan>,
    hub: Arc<sentry::Hub>,
}

async fn start_sentry_http_transaction(rqctx: Arc<RequestContext<Context>>) -> SentryTransaction {
    // Create a new Sentry hub for every request.
    // Ensures the scope stays right.
    // The Clippy lint here is a false positive, the suggestion to write
    // `Hub::with(Hub::new_from_top)` does not compiles:
    //     143 |         Hub::with(Hub::new_from_top).into()
    //         |         ^^^^^^^^^ implementation of `std::ops::FnOnce` is not general enough
    #[allow(clippy::redundant_closure)]
    let hub = Arc::new(Hub::with(|hub| Hub::new_from_top(hub)));

    // Get the raw headers.
    let raw_req = rqctx.request.lock().await;
    let raw_headers = raw_req.headers().clone();

    let sentry_req = sentry::protocol::Request {
        method: Some(raw_req.method().to_string()),
        url: raw_req.uri().to_string().parse().ok(),
        headers: raw_headers
            .iter()
            .map(|(header, value)| (header.to_string(), value.to_str().unwrap_or_default().into()))
            .collect(),
        ..Default::default()
    };

    let headers = raw_headers
        .iter()
        .flat_map(|(header, value)| value.to_str().ok().map(|value| (header.as_str(), value)));

    let tx_name = format!("{} {}", raw_req.method(), raw_req.uri().path());

    let trx_ctx = sentry::TransactionContext::continue_from_headers(&tx_name, "http.server", headers);

    hub.configure_scope(|scope| {
        scope.add_event_processor(move |mut event| {
            if event.request.is_none() {
                event.request = Some(sentry_req.clone());
            }
            Some(event)
        });
    });

    let transaction: sentry::TransactionOrSpan = sentry::start_transaction(trx_ctx).into();

    let mut trx = SentryTransaction {
        transaction: transaction.clone(),
        parent_span: None,
        hub: hub.clone(),
    };

    hub.configure_scope(|scope| {
        trx.parent_span = scope.get_span();
        scope.set_span(Some(transaction.clone()));
    });

    trx
}

impl SentryTransaction {
    pub fn finish(&mut self, status: StatusCode) -> () {
        if self.transaction.get_status().is_none() {
            let s = map_status(status);
            self.transaction.set_status(s);
        }
        self.transaction.clone().finish();

        if let Some(parent_span) = &self.parent_span {
            self.hub.configure_scope(|scope| {
                scope.set_span(Some(parent_span.clone()));
            });
        }

        ()
    }
}

fn map_status(status: StatusCode) -> protocol::SpanStatus {
    match status {
        StatusCode::UNAUTHORIZED => protocol::SpanStatus::Unauthenticated,
        StatusCode::FORBIDDEN => protocol::SpanStatus::PermissionDenied,
        StatusCode::NOT_FOUND => protocol::SpanStatus::NotFound,
        StatusCode::TOO_MANY_REQUESTS => protocol::SpanStatus::ResourceExhausted,
        status if status.is_client_error() => protocol::SpanStatus::InvalidArgument,
        StatusCode::NOT_IMPLEMENTED => protocol::SpanStatus::Unimplemented,
        StatusCode::SERVICE_UNAVAILABLE => protocol::SpanStatus::Unavailable,
        status if status.is_server_error() => protocol::SpanStatus::InternalError,
        StatusCode::CONFLICT => protocol::SpanStatus::AlreadyExists,
        status if status.is_success() => protocol::SpanStatus::Ok,
        _ => protocol::SpanStatus::UnknownError,
    }
}
