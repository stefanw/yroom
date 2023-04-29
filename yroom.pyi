from typing import Dict, List, Optional, TypedDict

class YRoomMessage:
    """
    Container that holds two members: `payload` and `broadcast_payload`.
    `payload` is a message that should be sent to the connection that sent the message.
    `broadcast_payload` is a message that should be sent to all connections in the room.
    Either or both of the members can be of zero length and then must not be sent.
    """

    payload: bytes
    broadcast_payload: bytes

class YRoomSettings(TypedDict):
    wire_version: int  # The Yjs encoding/decoding version to use (1 or 2, default: 1)
    name_prefixed: bool  # Whether to expect a prefix for wire messages (default: False)
    server_start_sync: bool  # Whether to start sync on connect (default: True)

class YRoomManager:
    def __init__(self, settings: Optional[Dict[str, YRoomSettings]]) -> None:
        """
        Create a new YRoomManager with an optional settings dict.
        """
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
    def get_map(self, room: str, name: str) -> Optional[str]:
        """
        Return the named map from the doc of that room as a JSON string or
        None if map or room does not exist.
        """
    def get_array(self, room: str, name: str) -> Optional[str]:
        """
        Return the named array from the doc of that room as a JSON string or
        None if array or room does not exist.
        """
    def get_text(self, room: str, name: str) -> Optional[str]:
        """
        Return the named text from the doc of that room or
        None if text or room does not exist.
        """
    def get_xml_element(self, room: str, name: str) -> Optional[str]:
        """
        Return the named xml element from the doc of that room as serialized string or
        None if xml element or room does not exist.
        """
    def get_xml_text(self, room: str, name: str) -> Optional[str]:
        """
        Return the named xml text from the doc of that room as serialized string or
        None if xml text or room does not exist.
        """
    def get_xml_fragment(self, room: str, name: str) -> Optional[str]:
        """
        Return the named xml fragment from the doc of that room as serialized string or
        None if xml fragment or room does not exist.
        """
