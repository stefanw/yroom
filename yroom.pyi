from typing import List

class YRoomMessage:
    """
    Container that holds two members: `payload` and `broadcast_payload`.
    `payload` is a message that should be sent to the connection that sent the message.
    `broadcast_payload` is a message that should be sent to all connections in the room.
    Either or both of the members can be of zero length and then must not be sent.
    """

    payload: bytes
    broadcast_payload: bytes

class YRoomManager:
    def connect(self, room: str, conn_id: int) -> YRoomMessage:
        """
        Connect to a room with a connection id, returning a YRoomMessage.
        """
    def connect_with_data(self, room: str, conn_id: int, data: bytes) -> YRoomMessage:
        """
        Connect to a room with a connection id and bytes of serialized Yjs document in
        `data`. The `data` will be used to initialize the Yjs document in the room if
        the room needs to be created.
        Returns a YRoomMessage.
        """
    def handle_message(self, room: str, conn_id: int, payload: bytes) -> YRoomMessage:
        """
        Handle a Yjs protocol message from a connection in a room.
        Returns a YRoomMessage.
        """
    def disconnect(self, room: str, conn_id: int) -> YRoomMessage:
        """
        Disconnect a connection from a room and removes the associated client from the
        awareness.
        Returns a YRoomMessage.
        """
    def has_room(self, room: str) -> bool:
        """
        Returns True if the room exists, otherwise False.
        """
    def is_room_alive(self, room: str) -> bool:
        """
        Returns True if the room exists and has connections, otherwise False.
        """
    def serialize_room(self, room: str) -> bytes:
        """
        Encode the document of the room as an Yjs update in bytes.
        """
    def remove_room(self, room: str) -> None:
        """
        Remove the room, dropping the document, awareness and connection mapping.
        """
    def list_rooms(self) -> List[str]:
        """
        Return list all room names that are available.
        """
