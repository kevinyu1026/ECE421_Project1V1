//! Database module to handle player registration, player stats, and login using SQLite
use crate::lobby::Player;
use sqlx::{SqlitePool, Row};
use uuid::Uuid;
use std::sync::Arc;


#[derive(Debug)]
pub struct PlayerStats {
    pub id: String,
    pub name: String,
    pub games_played: i32,
    pub games_won: i32,
    pub wallet: i32,
}

// #[derive(Debug)]
// pub struct Player {
//     pub id: String,
//     pub name: String,
//     pub games_played: i32,
//     pub games_won: i32,
//     pub wallet: i32,
// }

#[derive(Clone)]
pub struct Database {
    pub pool: Arc<SqlitePool>,
}

impl Database {
    pub fn new(pool: SqlitePool) -> Self {
        Database {
            pool: Arc::new(pool),
        }
    }

    pub async fn register_player(&self, name: &str) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        let wallet: u32 = 1000;
        sqlx::query("INSERT INTO players (id, name, wallet) VALUES (?1, ?2, ?3)")
            .bind(&id)
            .bind(name)
            .bind(&wallet)
            .execute(&*self.pool)
            .await?;
        Ok(id)
    }

    pub async fn login_player(&self, name: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query("SELECT id FROM players WHERE name = ?1")
            .bind(name)
            .fetch_optional(&*self.pool)
            .await?;
        Ok(row.map(|r| r.get(0)))
    }


    pub async fn player_stats(&self, username: &str) -> Result<PlayerStats, sqlx::Error> {
        let row = sqlx::query(
            "SELECT games_played, games_won, wallet FROM players WHERE name = ?1",
        )
        .bind(username)
        .fetch_one(&*self.pool)
        .await?;

        Ok(PlayerStats {
            games_played: row.get(0),
            games_won: row.get(1),
            wallet: row.get(2),
            id: Uuid::new_v4().to_string(),
            name: username.to_string(),
        })
    }

    pub async fn get_player_wallet(&self, username: &str) -> Result<i32, sqlx::Error> {
        let row = sqlx::query("SELECT wallet FROM players WHERE name = ?1")
            .bind(username)
            .fetch_one(&*self.pool)
            .await?;
        Ok(row.get(0))
    }

    pub async fn update_player_stats(&self, player: &Player) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE players SET games_played = games_played + ?1, games_won = games_won + ?2, wallet = ?3 WHERE name = ?4",
        )
        .bind(player.games_played)
        .bind(player.games_won)
        .bind(player.wallet)
        .bind(&player.name)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_database() -> Database {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE players (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                games_played INTEGER DEFAULT 0,
                games_won INTEGER DEFAULT 0,
                wallet INTEGER DEFAULT 1000
            )"
        )
        .execute(&pool)
        .await
        .unwrap();

        Database::new(pool)
    }

    #[tokio::test]

    ///S-PR-3 Unit Test S-FR-3
    async fn test_statistics_across_multiple_instantiations() {
        let db = setup_database().await;

        // Register a player
        let player_name = "test_player";

        // Register a player
        db.register_player(player_name).await.unwrap();

        // Update player stats directly using SQL query
        for _ in 0..100 {
            sqlx::query(
                "UPDATE players SET games_played = games_played + 1, games_won = games_won + 1, wallet = 1000 WHERE name = ?1",
            )
            .bind(player_name)
            .execute(&*db.pool)
            .await
            .unwrap();
        }

        // Check player stats
        let stats = db.player_stats(player_name).await.unwrap();
        assert_eq!(stats.games_played, 100);
        assert_eq!(stats.games_won, 100);
        assert_eq!(stats.wallet, 1000);
    }

    //Test of testing where a statistic is reported to player using the username
    //S-FR-4
    #[tokio::test]
    async fn test_player_stats() {
        let db = setup_database().await;

        // Register a player
        let player_name = "test_player";
        db.register_player(player_name).await.unwrap();

        // Check player stats
        let stats = db.player_stats(player_name).await.unwrap();
        assert_eq!(stats.games_played, 0);
        assert_eq!(stats.games_won, 0);
        assert_eq!(stats.wallet, 1000);

    }
    // Test to check if all players have a unique username (no duplicates)
    #[tokio::test]
    async fn test_unique_username() {
        let db = setup_database().await;

        // Register a player
        let player_name = "unique_player";
        db.register_player(player_name).await.unwrap();

        // Attempt to register another player with the same name
        let result = db.register_player(player_name).await;

        // Check that the second registration attempt fails
        assert!(result.is_err());
    }
}


