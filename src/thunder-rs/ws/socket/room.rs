// server/ws/room.rs
use serde::Serialize;
use serde_json::to_vec;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::ws::opcode::Opcode;

use super::socket::Socket;

/// Room functionality for WebSocket connections
/// Enables grouping connections and broadcasting messages within rooms
/// This might be changed to a contain custom Data field in the future for more flexibility
pub struct Room {
    pub id: String,
    pub connections: HashMap<String, Arc<Socket>>,
}

impl Socket {
    /// Checks if a room with the specified ID exists.
    pub async fn room_exists(
        &self,
        rooms: &Arc<Mutex<HashMap<String, Room>>>,
        room_id: &str,
    ) -> bool {
        let rooms = rooms.lock().await;
        rooms.contains_key(room_id)
    }
    /// Gets all connection IDs in a room, optionally excluding one connection 
    pub async fn connections_vec(
        &self,
        rooms: &Arc<Mutex<HashMap<String, Room>>>,
        room_id: &str,
        c_id: Option<&String>,
    ) -> Vec<String> {
        let rooms = rooms.lock().await;
        let room = rooms.get(room_id).unwrap();
        room.connections
            .keys()
            .filter(|&key| Some(key) != c_id)
            .cloned()
            .collect()
    }
    /// Gets a specific socket from a room by connection ID
    pub async fn get_socket(
        &self,
        rooms: &Arc<Mutex<HashMap<String, Room>>>,
        room_id: &str,
        c_id: &str,
    ) -> Option<Arc<Socket>> {
        let rooms = rooms.lock().await;
        let room = rooms.get(room_id)?;
        room.connections.get(c_id).cloned()
    }
    /// Finds which room contains a specific connection ID
    pub async fn find_roomid(
        &self,
        rooms: &Arc<Mutex<HashMap<String, Room>>>,
        c_id: &str,
    ) -> Option<String> {
        let rooms = rooms.lock().await;
        for (room_id, room) in rooms.iter() {
            if room.connections.contains_key(c_id) {
                return Some(room_id.clone());
            }
        }
        None
    }
    /// Verifies if a connection exists in a specific room
    pub async fn check_id_in_room(
        rooms: &Arc<Mutex<HashMap<String, Room>>>,
        room_id: &str,
        connection_id: &str,
    ) -> bool {
        let rooms = rooms.lock().await;
        if let Some(room) = rooms.get(room_id) {
            if room.connections.contains_key(connection_id) {
                return true;
            }
        }
        false
    }
    /// Removes current connection from a room
    pub async fn remove_ws_from_room(
        &self,
        rooms: &Arc<Mutex<HashMap<String, Room>>>,
        room_id: &str,
    ) {
        let mut rooms = rooms.lock().await;
        if let Some(room) = rooms.get_mut(room_id) {
            room.connections.remove(&self.id);
        }
    }
    /// Broadcasts raw message to all room members except sender
    pub async fn broadcast_to_room(
        &self,
        rooms: &Arc<Mutex<HashMap<String, Room>>>,
        room_id: &str,
        opcode: Opcode,
        payload: &[u8],
    ) {
        let rooms = rooms.lock().await;
        if let Some(room) = rooms.get(room_id) {
            for connection in room.connections.values() {
                if connection.id != self.id {
                    connection.send(opcode.clone(), payload).await.unwrap();
                }
            }
        }
    }
    /// Creates new room if ID doesn't exist
    pub async fn create_room(
        &self,
        rooms: &Arc<Mutex<HashMap<String, Room>>>,
        room_id: &str,
    ) -> Result<(), String> {
        if self.room_exists(rooms, room_id).await {
            return Err(format!("Room with ID {room_id} already exists"));
        }

        let room = Room {
            id: room_id.to_string(),
            connections: HashMap::new(),
        };
        let mut rooms = rooms.lock().await;
        rooms.insert(room_id.to_string(), room);
        Ok(())
    }

    /// Removes room and all its connections
    pub async fn remove_room(&self, rooms: &Arc<Mutex<HashMap<String, Room>>>, room_id: &str) {
        let mut rooms = rooms.lock().await;
        rooms.remove(room_id);
    }
    /// Adds connection to existing room
    /// Returns error if room doesn't exist or connection is already in room
    pub async fn insert_ws_to_room(
        connection: &Arc<Socket>,
        rooms: &Arc<Mutex<HashMap<String, Room>>>,
        room_id: &str,
    ) -> Result<(), String> {
        let mut rooms = rooms.lock().await;
        if let Some(room) = rooms.get_mut(room_id) {
            if room.connections.contains_key(&connection.id) {
                return Err(format!(
                    "Connection with ID {} already exists in room {}",
                    connection.id, room_id
                ));
            }
            room.connections
                .insert(connection.id.clone(), Arc::clone(connection));
            Ok(())
        } else {
            Err(format!("Room with ID {room_id} does not exist"))
        }
    }
    /// Broadcasts JSON message to all room members except sender
    /// Handles serialization and proper WebSocket text frame formatting
    pub async fn broadcast_json_to_room<T: Serialize>(
        &self,
        rooms: &Arc<Mutex<HashMap<String, Room>>>,
        room_id: &str,
        data: &T,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let json_payload = to_vec(data)?;

        let rooms = rooms.lock().await;
        if let Some(room) = rooms.get(room_id) {
            for connection in room.connections.values() {
                if connection.id != self.id {
                    connection.send(Opcode::Text, &json_payload).await?;
                }
            }
        }
        Ok(())
    }
}
