pub use sea_orm_migration::prelude::*;

mod m20260101_000001_create_posts_table;
mod m20260102_000001_create_setup_tables;
mod m20260103_000001_create_sessions_table;
mod m20260104_000001_create_taxonomy_tables;
mod m20260105_000001_create_media_table;
mod m20260106_000001_create_post_taxonomy_tables;
mod m20260107_000001_create_plugin_settings;
mod m20260108_000001_add_content_languages;
mod m20260109_000001_add_content_markdown;
mod m20260110_000001_create_forms;
mod m20260111_000001_form_notify;
mod m20260112_000001_mailing;
mod m20260113_000001_campaign_sender;

pub struct Migrator;

#[sea_orm_migration::async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260101_000001_create_posts_table::Migration),
            Box::new(m20260102_000001_create_setup_tables::Migration),
            Box::new(m20260103_000001_create_sessions_table::Migration),
            Box::new(m20260104_000001_create_taxonomy_tables::Migration),
            Box::new(m20260105_000001_create_media_table::Migration),
            Box::new(m20260106_000001_create_post_taxonomy_tables::Migration),
            Box::new(m20260107_000001_create_plugin_settings::Migration),
            Box::new(m20260108_000001_add_content_languages::Migration),
            Box::new(m20260109_000001_add_content_markdown::Migration),
            Box::new(m20260110_000001_create_forms::Migration),
            Box::new(m20260111_000001_form_notify::Migration),
            Box::new(m20260112_000001_mailing::Migration),
            Box::new(m20260113_000001_campaign_sender::Migration),
        ]
    }
}
