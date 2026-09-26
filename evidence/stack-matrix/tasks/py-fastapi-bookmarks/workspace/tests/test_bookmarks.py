import pytest
from fastapi.testclient import TestClient

from app.main import create_app


@pytest.fixture
def client():
    return TestClient(create_app())


def add(client, url, **fields):
    response = client.post("/bookmarks", json={"url": url, **fields})
    assert response.status_code == 201, response.text
    return response.json()


def test_create_fills_title_and_normalises_tags(client):
    body = add(client, "https://docs.python.org/3/library/sqlite3.html", tags=[" Python", "python", "DB", ""])
    assert body == {
        "id": 1,
        "url": "https://docs.python.org/3/library/sqlite3.html",
        "title": "docs.python.org",
        "tags": ["db", "python"],
        "created": 1,
    }


def test_invalid_urls_are_rejected(client):
    for url in ["ftp://x.org/f", "not a url", "https://", ""]:
        assert client.post("/bookmarks", json={"url": url}).status_code == 422, url
    assert client.post("/bookmarks", json={}).status_code == 422


def test_duplicate_url_is_a_conflict(client):
    first = add(client, "https://example.com/a")
    response = client.post("/bookmarks", json={"url": "https://example.com/a", "title": "again"})
    assert response.status_code == 409
    assert response.json() == {"detail": "duplicate url", "id": first["id"]}


def test_get_and_404(client):
    created = add(client, "https://example.com/a", title="A")
    assert client.get(f"/bookmarks/{created['id']}").json() == created
    missing = client.get("/bookmarks/99")
    assert missing.status_code == 404
    assert "detail" in missing.json()


def test_list_is_newest_first_and_filters_combine(client):
    add(client, "https://a.example/rust", title="Rust book", tags=["rust"])
    add(client, "https://b.example/py", title="Python tips", tags=["python", "tips"])
    add(client, "https://c.example/py2", title="More Python", tags=["Python"])
    listed = client.get("/bookmarks").json()
    assert [b["id"] for b in listed] == [3, 2, 1]
    assert [b["id"] for b in client.get("/bookmarks", params={"tag": "PYTHON"}).json()] == [3, 2]
    assert [b["id"] for b in client.get("/bookmarks", params={"q": "tips", "tag": "python"}).json()] == [2]
    assert [b["id"] for b in client.get("/bookmarks", params={"q": "A.EXAMPLE"}).json()] == [1]


def test_paging_and_total(client):
    for n in range(25):
        add(client, f"https://example.com/{n}")
    response = client.get("/bookmarks", params={"limit": 10, "offset": 20})
    assert response.headers["X-Total-Count"] == "25"
    assert [b["id"] for b in response.json()] == [5, 4, 3, 2, 1]
    assert len(client.get("/bookmarks").json()) == 20
    assert client.get("/bookmarks", params={"limit": 0}).status_code == 422
    assert client.get("/bookmarks", params={"limit": 101}).status_code == 422
    assert client.get("/bookmarks", params={"offset": -1}).status_code == 422


def test_patch_changes_only_what_is_given(client):
    created = add(client, "https://example.com/a", title="A", tags=["x"])
    other = add(client, "https://example.com/b")
    response = client.patch(f"/bookmarks/{created['id']}", json={"tags": ["Y", "z"]})
    assert response.status_code == 200
    assert response.json() == {**created, "tags": ["y", "z"]}
    assert client.patch(f"/bookmarks/{created['id']}", json={"url": other["url"]}).status_code == 409
    assert client.patch(f"/bookmarks/{created['id']}", json={"url": "nope"}).status_code == 422
    assert client.patch("/bookmarks/99", json={"title": "x"}).status_code == 404


def test_delete_and_ids_are_not_reused(client):
    add(client, "https://example.com/a")
    second = add(client, "https://example.com/b")
    assert client.delete(f"/bookmarks/{second['id']}").status_code == 204
    assert client.delete(f"/bookmarks/{second['id']}").status_code == 404
    assert add(client, "https://example.com/c")["id"] == 3


def test_tags_are_counted(client):
    add(client, "https://example.com/a", tags=["python", "web"])
    add(client, "https://example.com/b", tags=["python"])
    add(client, "https://example.com/c", tags=["api", "web", "zeta"])
    assert client.get("/tags").json() == [
        {"tag": "python", "count": 2},
        {"tag": "web", "count": 2},
        {"tag": "api", "count": 1},
        {"tag": "zeta", "count": 1},
    ]


def test_apps_do_not_share_data():
    one, two = TestClient(create_app()), TestClient(create_app())
    add(one, "https://example.com/a")
    assert two.get("/bookmarks").json() == []
