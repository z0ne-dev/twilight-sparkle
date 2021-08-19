use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;
use tracing::*;
use twilight_gateway::cluster::Cluster;
use twilight_gateway::EventTypeFlags;
use twilight_http::Client as HttpClient;
use twilight_model::application::command::Command;
use twilight_model::gateway::event::Event;
use twilight_model::gateway::Intents;
use twilight_model::application::interaction::Interaction;
use twilight_model::application::callback::{InteractionResponse, CallbackData};


pub struct Context {
    pub http: HttpClient,
    pub cls: Cluster,
}

impl Context {
    fn new(
        http: HttpClient,
        cls: Cluster,
    ) -> Self {
        Self {
            http,
            cls,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let token = std::env::var("DISCORD_TOKEN")?;

    let http = HttpClient::new(token.clone());

    let current_user = http.current_user().exec().await?.model().await?;
    http.set_application_id(current_user.id.0.into());

    let commands = vec![
        Command {
            application_id: None,
            guild_id: None,
            name: "ping".to_owned(),
            default_permission: None,
            description: "Send a ping".to_owned(),
            id: None,
            options: vec![],
        }
    ];

    http.set_global_commands(&commands)?
        .exec()
        .await?;

    let intents = Intents::empty();
    let flags = EventTypeFlags::INTERACTION_CREATE;

    let (cluster, mut events) = Cluster::builder(&token, intents)
        .event_types(flags)
        .http_client(http.clone())
        .build()
        .await?;

    // spawn the cluster instance
    let cluster_spawn = cluster.clone();
    tokio::spawn(async move {
        cluster_spawn.up().await;
    });

    // context for the handler, could use typemap-rev for example as well
    let ctx = Arc::new(Context::new(
        http,
        cluster.clone(),
    ));

    // event dispatcher
    while let Some((_, event)) = events.next().await {
        let ctx_clone = ctx.clone();

        // spawn event handler
        tokio::spawn(handle_event(event, ctx_clone));
    }

    println!("closing down!");
    Ok(())
}

async fn handle_event(event: Event, ctx: Arc<Context>) -> Result<()> {
    match event {
        Event::InteractionCreate(interaction) => {
            match handle_slash(interaction.0, ctx).await {
                Ok(_) => (),
                Err(why) => warn!("Error handling slash: {}", why),
            }
            Ok(())
        }

        _ => Ok(()),
    }
}


pub async fn handle_slash(slash: Interaction, ctx: Arc<Context>) -> Result<()> {
    let slash = match slash {
        Interaction::Ping(_) => {
            warn!("Got slash ping!");
            return Ok(());
        }

        Interaction::ApplicationCommand(cmd) => cmd,

        // missing for now: message components

        unknown => {
            warn!("Got unhandled Interaction: {:?}", unknown);
            return Ok(());
        }
    };

    let name = slash.data.name.as_str();

    info!("Slash command used: {}", name);
    let response = match name {
        "ping" => InteractionResponse::ChannelMessageWithSource(CallbackData {
            allowed_mentions: None,
            content: Some("Pong!".to_owned()),
            embeds: vec![],
            flags: None,
            tts: None
        }),
        _ => {
            warn!("Unhandled command! {}", name);
            return Ok(());
        }
    };
    ctx.http
       .interaction_callback(slash.id, &slash.token, &response)
       .exec()
       .await?;
    Ok(())
}