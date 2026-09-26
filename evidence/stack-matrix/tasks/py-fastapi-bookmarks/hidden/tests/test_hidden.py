import sqlite3

from fastapi.testclient import TestClient

from app.main import create_app


def test_hidden_blank_title_falls_back_to_host():
    client = TestClient(create_app())
    body = client.post("/bookmarks", json={"url": "http://sub.example.org:8080/x?y=1", "title": "   "}).json()
    assert body["title"] == "sub.example.org"


def test_hidden_patch_title_to_blank_uses_the_host_of_the_current_url():
    client = TestClient(create_app())
    created = client.post("/bookmarks", json={"url": "https://a.example/x", "title": "Old"}).json()
    body = client.patch(f"/bookmarks/{created['id']}", json={"url": "https://b.example/y", "title": ""}).json()
    assert body["url"] == "https://b.example/y"
    assert body["title"] == "b.example"


def test_hidden_patch_to_its_own_url_is_fine():
    client = TestClient(create_app())
    created = client.post("/bookmarks", json={"url": "https://a.example/x"}).json()
    response = client.patch(f"/bookmarks/{created['id']}", json={"url": "https://a.example/x", "title": "T"})
    assert response.status_code == 200
    assert response.json()["title"] == "T"


def test_hidden_deleted_tags_disappear_from_counts():
    client = TestClient(create_app())
    created = client.post("/bookmarks", json={"url": "https://a.example/x", "tags": ["gone"]}).json()
    client.post("/bookmarks", json={"url": "https://b.example/x", "tags": ["kept"]})
    client.delete(f"/bookmarks/{created['id']}")
    assert client.get("/tags").json() == [{"tag": "kept", "count": 1}]


def test_hidden_file_database_persists_between_apps(tmp_path):
    path = str(tmp_path / "bookmarks.sqlite")
    first = TestClient(create_app(path))
    first.post("/bookmarks", json={"url": "https://a.example/x", "tags": ["t"]})
    second = TestClient(create_app(path))
    listed = second.get("/bookmarks").json()
    assert [b["url"] for b in listed] == ["https://a.example/x"]
    assert second.post("/bookmarks", json={"url": "https://b.example/x"}).json()["id"] == 2
    assert sqlite3.connect(path).execute("select count(*) from sqlite_master").fetchone()[0] > 0


def test_hidden_offset_past_the_end_is_empty_with_total():
    client = TestClient(create_app())
    client.post("/bookmarks", json={"url": "https://a.example/x"})
    response = client.get("/bookmarks", params={"offset": 5})
    assert response.status_code == 200
    assert response.json() == []
    assert response.headers["X-Total-Count"] == "1"
