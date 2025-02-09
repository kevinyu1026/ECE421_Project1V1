//! Database module to handle player registration, player stats, and login using SQLite
use crate::Player;
use sqlx::{SqlitePool, Row};
use uuid::Uuid;
use std::sync::Arc;

#[derive(Debug)]
pub struct PlayerStats {
    pub games_played: i32,
    pub games_won: i32,
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
            "SELECT games_played, games_won FROM players WHERE name = ?1",
        )
        .bind(username)
        .fetch_one(&*self.pool)
        .await?;

        Ok(PlayerStats {
            games_played: row.get(0),
            games_won: row.get(1),
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


    
