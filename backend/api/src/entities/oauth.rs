//! The three tables an authorization flow needs. See the migration for why
//! there is no table here for the access token itself.

pub mod client {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "oauth_clients")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: String,
        pub name: String,
        /// Newline-separated. A redirect that is not one of these is refused.
        pub redirect_uris: String,
        pub created_at: DateTimeWithTimeZone,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}

    impl Model {
        pub fn may_redirect_to(&self, uri: &str) -> bool {
            self.redirect_uris.lines().any(|allowed| allowed == uri)
        }
    }
}

pub mod code {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "oauth_codes")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub code: String,
        pub client_id: String,
        pub user_id: Uuid,
        pub redirect_uri: String,
        pub challenge: String,
        pub expires_at: DateTimeWithTimeZone,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod grant {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "oauth_grants")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        pub client_id: String,
        pub user_id: Uuid,
        /// The session handed out as the access token.
        pub session_id: Uuid,
        pub refresh: String,
        pub created_at: DateTimeWithTimeZone,
        /// When the connection was last renewed, which is the closest thing
        /// there is to "when was this last used".
        pub used_at: DateTimeWithTimeZone,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
