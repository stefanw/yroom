import json

import y_py as Y

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
    d1 = Y.YDoc()
    text = d1.get_text("test")
    with d1.begin_transaction() as txn:
        text.extend(txn, "hello world!")
    update_data = Y.encode_state_as_update(d1)

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


def test_extraction():
    d1 = Y.YDoc()
    text = d1.get_text("text")
    test_text = "hello world!"
    array = d1.get_array("array")
    test_array = [1, "foo", True]
    map = d1.get_map("map")
    test_map = {"a": 1}
    xml_element = d1.get_xml_element("xml_element")
    with d1.begin_transaction() as txn:
        text.extend(txn, test_text)
        array.extend(txn, test_array)
        map.update(txn, test_map)

        b = xml_element.push_xml_text(txn)
        a = xml_element.insert_xml_element(txn, 0, "p")
        aa = a.push_xml_text(txn)
        aa.push(txn, "hello")
        b.push(txn, "world")

    update_data = Y.encode_state_as_update(d1)

    room_name = "test"
    manager = YRoomManager()
    manager.connect_with_data(room_name, 1, update_data)
    assert manager.export_text(room_name, "text") == test_text
    assert json.loads(manager.export_array(room_name, "array")) == test_array
    assert json.loads(manager.export_map(room_name, "map")) == test_map
    assert (
        manager.export_xml_element(room_name, "xml_element")
        == "<UNDEFINED><p>hello</p>world</UNDEFINED>"
    )


def test_server_sync():
    d1 = Y.YDoc()
    text = d1.get_text("test")
    with d1.begin_transaction() as txn:
        text.extend(txn, "hello world!")

    room_name = "test"
    client_id = 1
    manager = YRoomManager()
    message = manager.connect(room_name, client_id)
    initial_payload = b"".join(
        [
            b"\x00\x00",  # sync step 1
            b"\x01" b"\x00",  # len message  # zero length state vector
        ]
    )
    assert message.payload == initial_payload
    with d1.begin_transaction() as txn:
        diff = txn.diff_v1(None)

    payload = b"".join(
        [
            b"\x00\x01",  # sync step 2
            len(diff).to_bytes(1, "big"),  # len of diff
            diff,  # the diff
        ]
    )
    message = manager.handle_message(room_name, client_id, payload)
    assert message.payload == b""
    assert message.broadcast_payload == b""
    assert manager.export_text(room_name, "test") == "hello world!"


def test_server_no_sync_start():
    empty = Y.YDoc()
    d1 = Y.YDoc()
    text = d1.get_text("test")
    with d1.begin_transaction() as txn:
        text.extend(txn, "hello world!")

    room_name = "test"
    client_id = 1
    manager = YRoomManager({room_name: {"SERVER_START_SYNC": False}})
    message = manager.connect(room_name, client_id)
    assert message.payload == b""
    assert message.broadcast_payload == b""

    state_vector = Y.encode_state_vector(d1)
    sv_len = len(state_vector).to_bytes(1, "big")
    client_sync_step1_payload = b"".join(
        [
            b"\x00\x00",  # sync step 1
            sv_len,
            state_vector,
        ]
    )
    message = manager.handle_message(room_name, client_id, client_sync_step1_payload)

    # Simulate empty document diff with d1
    with empty.begin_transaction() as txn:
        diff = txn.diff_v1(state_vector)
    len_diff = len(diff).to_bytes(1, "big")

    assert message.payload == b"".join(
        [
            b"\x00\x01",  # sync step 2
            len_diff,  # len of buffer
            diff,  # diffed update
        ]
    )
    assert message.broadcast_payload == b""


def test_client_prefix():
    """TipTap HocusPocus Collaboration uses a prefix in protocol messages"""
    d1 = Y.YDoc()
    name = "test"
    prefix = b"".join([len(name).to_bytes(1, "big"), name.encode("utf-8")])
    text = d1.get_text(name)
    with d1.begin_transaction() as txn:
        text.extend(txn, "hello world!")

    room_name = "test"
    client_id = 1
    manager = YRoomManager(
        {
            room_name: {
                "SERVER_START_SYNC": False,
                "PROTOCOL_NAME_PREFIX": True,
            }
        }
    )
    message = manager.connect(room_name, client_id)
    assert message.payload == b""
    assert message.broadcast_payload == b""

    state_vector = Y.encode_state_vector(d1)
    sv_len = len(state_vector).to_bytes(1, "big")
    client_sync_step1_payload = b"".join(
        [
            prefix,
            b"\x00\x00",  # sync step 1
            sv_len,
            state_vector,
        ]
    )

    message = manager.handle_message(room_name, client_id, client_sync_step1_payload)
    assert message.payload == b"".join(
        [
            prefix,
            b"\x00\x01",  # sync step 2
            b"\x02",  # len of buffer
            b"\x00\x00",  # diffed update
        ]
    )

    assert message.broadcast_payload == b""
