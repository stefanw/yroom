use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use pyo3::{prelude::*, types::PyBytes};

use lib0::decoding::Cursor;
use y_sync::{
    awareness::Awareness,
    sync::{Message, MessageReader, SyncMessage},
};
use yrs::{
    updates::{
        decoder::{Decode, DecoderV1},
        encoder::{Encode, Encoder, EncoderV1},
    },
    ReadTxn, StateVector, Transact, Update,
};

#[pyclass]
pub struct YRoomMessage {
    #[pyo3(get)]
    pub payload: PyObject,
    #[pyo3(get)]
    pub broadcast_payload: PyObject,
}

#[pyclass]
pub struct YRoomManager {
    rooms: HashMap<String, YRoom>,
}

impl YRoomManager {
    fn get_room_with_data(&mut self, room: &str, data: Vec<u8>) -> &mut YRoom {
        self.rooms
            .entry(room.to_string())
            .or_insert_with(|| YRoom::new(Some(data)))
    }

    fn get_room(&mut self, room: &str) -> &mut YRoom {
        self.rooms
            .entry(room.to_string())
            .or_insert_with(|| YRoom::new(None))
    }
}

#[pymethods]
impl YRoomManager {
    #[new]
    fn new() -> Self {
        YRoomManager {
            rooms: HashMap::new(),
        }
    }

    pub fn connect(&mut self, room: String, conn_id: u64) -> YRoomMessage {
        self.get_room(&room).connect(conn_id)
    }
    pub fn connect_with_data(&mut self, room: String, conn_id: u64, data: Vec<u8>) -> YRoomMessage {
        self.get_room_with_data(&room, data).connect(conn_id)
    }

    pub fn handle_message(&mut self, room: String, conn_id: u64, payload: Vec<u8>) -> YRoomMessage {
        self.get_room(&room).handle_message(conn_id, payload)
    }

    pub fn disconnect(&mut self, room: String, conn_id: u64) -> YRoomMessage {
        let broadcast_payload = self.get_room(&room).disconnect(conn_id);
        Python::with_gil(|py| YRoomMessage {
            payload: PyBytes::new(py, &[]).into(),
            broadcast_payload: PyBytes::new(py, &broadcast_payload).into(),
        })
    }

    pub fn has_room(&self, room: String) -> bool {
        self.rooms.contains_key(&room)
    }

    pub fn is_room_alive(&self, room: String) -> bool {
        let room = self.rooms.get(&room);
        match room {
            Some(room) => room.is_alive(),
            None => false,
        }
    }

    pub fn serialize_room(&self, room: String) -> PyObject {
        let yroom = self.rooms.get(&room);
        Python::with_gil(|py| match yroom {
            None => py.None(),
            Some(yroom) => PyBytes::new(py, &yroom.serialize()).into(),
        })
    }

    pub fn remove_room(&mut self, room: String) {
        self.rooms.remove(&room);
    }

    pub fn list_rooms(&self) -> Vec<String> {
        self.rooms.keys().cloned().collect()
    }
}

pub struct YRoom {
    awareness: Awareness,
    connections: Arc<Mutex<HashMap<u64, HashSet<u64>>>>,
}

impl YRoom {
    fn new(update_vec: Option<Vec<u8>>) -> Self {
        log::info!("Creating new YRoom");
        let mut awareness = Awareness::default();
        if let Some(update_vec) = update_vec {
            let update = Update::decode_v1(&update_vec);
            match update {
                Ok(update) => {
                    let mut txn = awareness.doc_mut().transact_mut();
                    txn.apply_update(update);
                }
                Err(e) => log::error!("Error decoding update: {}", e),
            }
        }
        YRoom {
            awareness,
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn connect(&mut self, conn_id: u64) -> YRoomMessage {
        let connections = self.connections.lock();
        connections
            .unwrap()
            .entry(conn_id)
            .or_insert_with(HashSet::new);

        let sv = self.awareness.doc().transact().state_vector();
        let mut encoder = EncoderV1::new();
        Message::Sync(SyncMessage::SyncStep1(sv)).encode(&mut encoder);

        if let Ok(awareness_update) = self.awareness.update() {
            Message::Awareness(awareness_update).encode(&mut encoder);
        }

        let payload = encoder.to_vec();
        Python::with_gil(|py| YRoomMessage {
            payload: PyBytes::new(py, &payload).into(),
            broadcast_payload: PyBytes::new(py, &[]).into(),
        })
    }

    pub fn handle_message(&mut self, conn_id: u64, payload: Vec<u8>) -> YRoomMessage {
        let mut sync_encoder = EncoderV1::new();
        let mut update_encoder = EncoderV1::new();
        let mut decoder = DecoderV1::new(Cursor::new(&payload));
        let reader = MessageReader::new(&mut decoder);
        reader.for_each(|message_result| match message_result {
            Ok(message) => match message {
                Message::Sync(SyncMessage::SyncStep1(sv)) => {
                    let txn = self.awareness.doc_mut().transact_mut();
                    let data = txn.encode_diff_v1(&sv);
                    let message = Message::Sync(SyncMessage::SyncStep2(data));
                    message.encode(&mut sync_encoder);
                }
                Message::Sync(SyncMessage::SyncStep2(data)) => {
                    let update = Update::decode_v1(&data);
                    match update {
                        Ok(update) => {
                            let mut txn = self.awareness.doc_mut().transact_mut();
                            txn.apply_update(update);
                        }
                        Err(e) => log::error!("Error decoding update: {}", e),
                    }
                }
                Message::Sync(SyncMessage::Update(data)) => {
                    let update = Update::decode_v1(&data);
                    match update {
                        Ok(update) => {
                            let mut txn = self.awareness.doc_mut().transact_mut();
                            txn.apply_update(update);
                            let message = Message::Sync(SyncMessage::Update(data));
                            message.encode(&mut update_encoder);
                        }
                        Err(e) => log::error!("Error decoding update: {}", e),
                    }
                }
                Message::Auth(_) => {}
                Message::AwarenessQuery => {
                    if let Ok(awareness_update) = self.awareness.update() {
                        Message::Awareness(awareness_update).encode(&mut sync_encoder);
                    }
                }
                Message::Awareness(awareness_update) => {
                    // Add/remove client ids to conn ids
                    self.connections
                        .lock()
                        .unwrap()
                        .entry(conn_id)
                        .or_insert_with(HashSet::new);
                    let connections = self.connections.clone();
                    {
                        let _sub = self.awareness.on_update(move |_, ev| {
                            let mut connections = connections.lock().unwrap();
                            let client_ids = connections.get_mut(&conn_id).unwrap();
                            ev.added().iter().for_each(|client_id| {
                                client_ids.insert(*client_id);
                            });
                            ev.removed().iter().for_each(|client_id| {
                                client_ids.remove(client_id);
                            });
                        });
                        if let Err(e) = self.awareness.apply_update(awareness_update) {
                            log::error!("Error applying awareness update: {}", e);
                        }
                    }
                    if let Ok(awareness_update) = self.awareness.update() {
                        Message::Awareness(awareness_update).encode(&mut update_encoder);
                    }
                }
                Message::Custom(_, _) => {}
            },
            _ => {
                log::warn!("Unknown message from connection {}", conn_id);
            }
        });

        Python::with_gil(|py| YRoomMessage {
            payload: PyBytes::new(py, &sync_encoder.to_vec()).into(),
            broadcast_payload: PyBytes::new(py, &update_encoder.to_vec()).into(),
        })
    }

    pub fn disconnect(&mut self, conn_id: u64) -> Vec<u8> {
        {
            let mut connections = self.connections.lock().unwrap();
            let client_ids = connections.get(&conn_id);
            if let Some(client_ids) = client_ids {
                client_ids.iter().for_each(|client_id| {
                    self.awareness.remove_state(*client_id);
                });
            }
            connections.remove(&conn_id);
        }
        let mut encoder = EncoderV1::new();
        if let Ok(awareness_update) = self.awareness.update() {
            Message::Awareness(awareness_update).encode(&mut encoder);
        }
        encoder.to_vec()
    }

    pub fn serialize(&self) -> Vec<u8> {
        let txn = self.awareness.doc().transact();
        txn.encode_state_as_update_v1(&StateVector::default())
    }

    pub fn is_alive(&self) -> bool {
        !self.connections.lock().unwrap().is_empty()
    }
}
