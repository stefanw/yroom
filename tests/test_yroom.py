from yroom import YRoomManager


def test_connect():
    room_name = "test"
    manager = YRoomManager()
    message = manager.connect(room_name, 1)
    assert len(message.payload) > 0
    assert len(message.broadcast_payload) == 0
    assert manager.has_room(room_name)
    assert manager.is_room_alive(room_name)
    assert manager.list_rooms() == [room_name]
    manager.remove_room(room_name)
    assert not manager.has_room(room_name)
    assert manager.list_rooms() == []


def test_connect_with_data():
    update_data = bytes(
        [
            1,
            3,
            227,
            214,
            245,
            198,
            5,
            0,
            4,
            1,
            4,
            116,
            121,
            112,
            101,
            1,
            48,
            68,
            227,
            214,
            245,
            198,
            5,
            0,
            1,
            49,
            68,
            227,
            214,
            245,
            198,
            5,
            1,
            1,
            50,
            0,
        ]
    )
    room_name = "test"
    manager = YRoomManager()
    message = manager.connect_with_data(room_name, 1, update_data)
    assert len(message.payload) > 0
    assert len(message.broadcast_payload) == 0
    assert manager.serialize_room(room_name) == update_data


def test_disconnect():
    room_name = "test"
    conn_id = 1
    manager = YRoomManager()
    manager.connect(room_name, conn_id)
    assert manager.has_room(room_name)
    assert manager.is_room_alive(room_name)
    message = manager.disconnect(room_name, conn_id)
    assert len(message.payload) == 0
    assert len(message.broadcast_payload) > 0
    assert not manager.is_room_alive(room_name)
    assert manager.list_rooms() == [room_name]
    manager.remove_room(room_name)
    assert not manager.has_room(room_name)
    assert manager.list_rooms() == []


def test_connect_multiple():
    room_name = "test"
    conn_id_1 = 1
    conn_id_2 = 2
    manager = YRoomManager()
    manager.connect(room_name, conn_id_1)
    manager.connect(room_name, conn_id_2)
    assert manager.is_room_alive(room_name)
    manager.disconnect(room_name, conn_id_1)
    assert manager.is_room_alive(room_name)
    manager.disconnect(room_name, conn_id_2)
    assert not manager.is_room_alive(room_name)
