use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use cio_api::{
    api_tokens::{APIToken, NewAPIToken},
    companies::Company,
    schema::api_tokens,
};
use diesel::{BoolExpressionMethods, ExpressionMethods, QueryDsl, RunQueryDsl};
use dropshot::{Query, RequestContext};
use google_drive::Client as GoogleDrive;
use gusto_api::Client as Gusto;
use mailchimp_api::MailChimp;
use ramp_api::Client as Ramp;
use slack_chat_api::Slack;
use zoom_api::Client as Zoom;

use crate::{AuthCallback, Context};

pub async fn handle_auth_google_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<()> {
    let event = query_args.into_inner();

    let api_context = rqctx.context();

    // Initialize the Google client.
    // You can use any of the libs here, they all use the same endpoint
    // for tokens and we will send all the scopes.
    let mut g = GoogleDrive::new_from_env("", "").await;

    // Let's get the token from the code.
    let t = g.get_access_token(&event.code, &event.state).await?;

    let client = reqwest::Client::new();

    // Let's get the company from information about the user.
    let mut headers = reqwest::header::HeaderMap::new();
    headers.append(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    headers.append(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", t.access_token))?,
    );

    let params = [("alt", "json")];
    let resp = client
        .get("https://www.googleapis.com/oauth2/v1/userinfo")
        .headers(headers)
        .query(&params)
        .send()
        .await?;

    // Get the response.
    let metadata: cio_api::companies::UserInfo = resp.json().await?;

    let company = Company::get_from_domain(&api_context.db, &metadata.hd)?;

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "google".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.refresh_token_expires_in as i32,
        company_id: metadata.hd.to_string(),
        item_id: "".to_string(),
        user_email: metadata.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: "".to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE, NO 1.
        cio_company_id: 1,
    };
    token.expand();

    // Update it in the database.
    token.upsert(&api_context.db).await?;

    Ok(())
}

pub async fn handle_auth_mailchimp_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<()> {
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    // Initialize the MailChimp client.
    let mut g = MailChimp::new_from_env("", "", "");

    // Let's get the token from the code.
    let t = g.get_access_token(&event.code).await?;

    // Let's get the metadata.
    let metadata = g.metadata().await?;

    // Let's get the domain from the email.
    let split = metadata.login.email.split('@');
    let vec: Vec<&str> = split.collect();
    let mut domain = "".to_string();
    if vec.len() > 1 {
        domain = vec.get(1).unwrap().to_string();
    }

    let company = Company::get_from_domain(&api_context.db, &domain)?;

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "mailchimp".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.x_refresh_token_expires_in as i32,
        company_id: metadata.accountname.to_string(),
        item_id: "".to_string(),
        user_email: metadata.login.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        // Format the endpoint with the dc.
        // https://${server}.api.mailchimp.com
        endpoint: metadata.api_endpoint.to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();
    // Update it in the database.
    token.upsert(&api_context.db).await?;

    Ok(())
}

pub async fn handle_auth_gusto_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<()> {
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    // Initialize the Gusto client.
    let mut g = Gusto::new_from_env("", "");

    // Let's get the token from the code.
    let t = g.get_access_token(&event.code, &event.state).await?;

    // Let's get the company ID.
    let current_user = g.current_user().get_me().await?;
    let mut company_id = String::new();
    if let Some(roles) = current_user.roles {
        if let Some(payroll_admin) = roles.payroll_admin {
            company_id = payroll_admin.companies.get(0).unwrap().id.to_string();
        }
    }

    // Let's get the domain from the email.
    let split = current_user.email.split('@');
    let vec: Vec<&str> = split.collect();
    let mut domain = "".to_string();
    if vec.len() > 1 {
        domain = vec.get(1).unwrap().to_string();
    }

    let company = Company::get_from_domain(&api_context.db, &domain)?;

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "gusto".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.refresh_token_expires_in as i32,
        company_id: company_id.to_string(),
        item_id: "".to_string(),
        user_email: current_user.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: "".to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();
    // Update it in the database.
    token.upsert(&api_context.db).await?;

    Ok(())
}

pub async fn handle_auth_zoom_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<()> {
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    // Initialize the Zoom client.
    let mut g = Zoom::new_from_env("", "");

    // Let's get the token from the code.
    let t = g.get_access_token(&event.code, &event.state).await?;

    let cu = g
        .users()
        .user(
            "me",
            zoom_api::types::LoginType::Noop, // We don't know the login type, so let's leave it empty.
            false,
        )
        .await?;

    // Let's get the domain from the email.
    let mut domain = "".to_string();
    if !cu.user.email.is_empty() {
        let split = cu.user.email.split('@');
        let vec: Vec<&str> = split.collect();
        if vec.len() > 1 {
            domain = vec.get(1).unwrap().to_string();
        }
    }

    let company = Company::get_from_domain(&api_context.db, &domain)?;

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "zoom".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.refresh_token_expires_in as i32,
        company_id: cu.user_response.company.to_string(),
        item_id: "".to_string(),
        user_email: cu.user.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: "".to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();
    // Update it in the database.
    token.upsert(&api_context.db).await?;

    Ok(())
}

pub async fn handle_auth_ramp_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<()> {
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    // Initialize the Ramp client.
    let mut g = Ramp::new_from_env("", "");

    // Let's get the token from the code.
    let t = g.get_access_token(&event.code, &event.state).await?;

    let ru = g
        .users()
        .get_all(
            "", // department id
            "", // location id
        )
        .await?;

    // Let's get the domain from the email.
    let mut domain = "".to_string();
    if !ru.is_empty() {
        let split = ru.get(0).unwrap().email.split('@');
        let vec: Vec<&str> = split.collect();
        if vec.len() > 1 {
            domain = vec.get(1).unwrap().to_string();
        }
    }

    let company = Company::get_from_domain(&api_context.db, &domain)?;

    // Save the token to the database.
    let mut token = NewAPIToken {
        product: "ramp".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: t.expires_in as i32,
        refresh_token: t.refresh_token.to_string(),
        refresh_token_expires_in: t.refresh_token_expires_in as i32,
        company_id: "".to_string(),
        item_id: "".to_string(),
        user_email: "".to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: "".to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();
    // Update it in the database.
    token.upsert(&api_context.db).await?;

    Ok(())
}

pub async fn handle_auth_slack_callback(
    rqctx: Arc<RequestContext<Context>>,
    query_args: Query<AuthCallback>,
) -> Result<()> {
    let api_context = rqctx.context();
    let event = query_args.into_inner();

    // Initialize the Slack client.
    let mut s = Slack::new_from_env("", "", "");

    // Let's get the token from the code.
    let t = s.get_access_token(&event.code).await?;

    // Get the current user.
    let current_user = s.current_user().await?;

    // Let's get the domain from the email.
    let split = current_user.email.split('@');
    let vec: Vec<&str> = split.collect();
    let mut domain = "".to_string();
    if vec.len() > 1 {
        domain = vec.get(1).unwrap().to_string();
    }

    let company = Company::get_from_domain(&api_context.db, &domain)?;

    let mut webhook = "".to_string();
    if let Some(wh) = t.incoming_webhook {
        webhook = wh.url;
    }

    // Save the bot token to the database.
    let mut token = NewAPIToken {
        product: "slack".to_string(),
        token_type: t.token_type.to_string(),
        access_token: t.access_token.to_string(),
        expires_in: 0,
        refresh_token: "".to_string(),
        refresh_token_expires_in: 0,
        company_id: t.team.id.to_string(),
        item_id: t.team.name.to_string(),
        user_email: current_user.email.to_string(),
        last_updated_at: Utc::now(),
        expires_date: None,
        refresh_token_expires_date: None,
        endpoint: webhook.to_string(),
        auth_company_id: company.id,
        company: Default::default(),
        // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
        cio_company_id: 1,
    };
    token.expand();

    // Update it in the database.
    let mut new_token = if let Ok(existing) = api_tokens::dsl::api_tokens
        .filter(
            api_tokens::dsl::cio_company_id
                .eq(1)
                .and(api_tokens::dsl::product.eq("slack".to_string()))
                .and(api_tokens::dsl::auth_company_id.eq(company.id))
                .and(api_tokens::dsl::token_type.eq(token.token_type.to_string())),
        )
        .first::<APIToken>(&api_context.db.conn())
    {
        diesel::update(&existing)
            .set(token)
            .get_result::<APIToken>(&api_context.db.conn())
            .unwrap_or_else(|e| panic!("unable to update record {}: {}", existing.id, e))
    } else {
        token.create_in_db(&api_context.db)?
    };
    new_token.upsert_in_airtable(&api_context.db).await?;

    // Save the user token to the database.
    if let Some(authed_user) = t.authed_user {
        let mut user_token = NewAPIToken {
            product: "slack".to_string(),
            token_type: authed_user.token_type.to_string(),
            access_token: authed_user.access_token.to_string(),
            expires_in: 0,
            refresh_token: "".to_string(),
            refresh_token_expires_in: 0,
            company_id: t.team.id.to_string(),
            item_id: t.team.name.to_string(),
            user_email: current_user.email.to_string(),
            last_updated_at: Utc::now(),
            expires_date: None,
            refresh_token_expires_date: None,
            endpoint: webhook.to_string(),
            auth_company_id: company.id,
            company: Default::default(),
            // THIS SHOULD ALWAYS BE OXIDE SO THAT IT SAVES TO OUR AIRTABLE.
            cio_company_id: 1,
        };
        user_token.expand();

        // Update it in the database.
        let mut new_user_token = if let Ok(existing) = api_tokens::dsl::api_tokens
            .filter(
                api_tokens::dsl::cio_company_id
                    .eq(1)
                    .and(api_tokens::dsl::product.eq("slack".to_string()))
                    .and(api_tokens::dsl::auth_company_id.eq(company.id))
                    .and(api_tokens::dsl::token_type.eq(user_token.token_type.to_string())),
            )
            .first::<APIToken>(&api_context.db.conn())
        {
            diesel::update(&existing)
                .set(user_token)
                .get_result::<APIToken>(&api_context.db.conn())
                .unwrap_or_else(|e| panic!("unable to update record {}: {}", existing.id, e))
        } else {
            user_token.create_in_db(&api_context.db)?
        };
        new_user_token.upsert_in_airtable(&api_context.db).await?;
    }

    Ok(())
}