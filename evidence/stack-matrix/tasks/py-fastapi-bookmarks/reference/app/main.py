import sqlite3
import threading
from typing import Optional
from urllib.parse import urlsplit

from fastapi import FastAPI, HTTPException, Query, Response
from fastapi.responses import JSONResponse
from pydantic import BaseModel


class BookmarkIn(BaseModel):
    url: str
    title: Optional[str] = None
    tags: list[str] = []


class BookmarkPatch(BaseModel):
    url: Optional[str] = None
    title: Optional[str] = None
    tags: Optional[list[str]] = None


def host_of(url: str) -> str:
    parts = urlsplit(url)
    if parts.scheme not in ("http", "https") or not parts.hostname:
        raise HTTPException(status_code=422, detail="url must be http(s) with a host")
    return parts.hostname


def clean_tags(tags: list[str]) -> list[str]:
    return sorted({tag.strip().lower() for tag in tags if tag.strip()})


def create_app(db_path: str = ":memory:") -> FastAPI:
    app = FastAPI()
    db = sqlite3.connect(db_path, check_same_thread=False)
    db.executescript(
        """
        CREATE TABLE IF NOT EXISTS bookmarks (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          url TEXT NOT NULL UNIQUE,
          title TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tags (
          bookmark_id INTEGER NOT NULL,
          tag TEXT NOT NULL,
          PRIMARY KEY (bookmark_id, tag)
        );
        """
    )
    lock = threading.Lock()

    def load(bookmark_id: int) -> dict:
        row = db.execute("SELECT id, url, title FROM bookmarks WHERE id = ?", (bookmark_id,)).fetchone()
        if row is None:
            raise HTTPException(status_code=404, detail="bookmark not found")
        tags = [t for (t,) in db.execute("SELECT tag FROM tags WHERE bookmark_id = ? ORDER BY tag", (row[0],))]
        return {"id": row[0], "url": row[1], "title": row[2], "tags": tags, "created": row[0]}

    def owner(url: str) -> Optional[int]:
        row = db.execute("SELECT id FROM bookmarks WHERE url = ?", (url,)).fetchone()
        return row[0] if row else None

    def set_tags(bookmark_id: int, tags: list[str]) -> None:
        db.execute("DELETE FROM tags WHERE bookmark_id = ?", (bookmark_id,))
        db.executemany("INSERT INTO tags VALUES (?, ?)", [(bookmark_id, t) for t in clean_tags(tags)])

    @app.post("/bookmarks", status_code=201)
    def create(bookmark: BookmarkIn):
        host = host_of(bookmark.url)
        with lock:
            existing = owner(bookmark.url)
            if existing is not None:
                return JSONResponse(status_code=409, content={"detail": "duplicate url", "id": existing})
            title = bookmark.title.strip() if bookmark.title and bookmark.title.strip() else host
            cursor = db.execute("INSERT INTO bookmarks (url, title) VALUES (?, ?)", (bookmark.url, title))
            set_tags(cursor.lastrowid, bookmark.tags)
            db.commit()
            return load(cursor.lastrowid)

    @app.get("/bookmarks")
    def listing(
        response: Response,
        tag: Optional[str] = None,
        q: Optional[str] = None,
        limit: int = Query(20, ge=1, le=100),
        offset: int = Query(0, ge=0),
    ):
        where, params = [], []
        if tag is not None:
            where.append("EXISTS (SELECT 1 FROM tags t WHERE t.bookmark_id = b.id AND t.tag = ?)")
            params.append(tag.strip().lower())
        if q is not None:
            where.append("(instr(lower(b.title), ?) > 0 OR instr(lower(b.url), ?) > 0)")
            params += [q.lower(), q.lower()]
        clause = f"WHERE {' AND '.join(where)}" if where else ""
        with lock:
            total = db.execute(f"SELECT count(*) FROM bookmarks b {clause}", params).fetchone()[0]
            ids = db.execute(
                f"SELECT b.id FROM bookmarks b {clause} ORDER BY b.id DESC LIMIT ? OFFSET ?", params + [limit, offset]
            ).fetchall()
            response.headers["X-Total-Count"] = str(total)
            return [load(i) for (i,) in ids]

    @app.get("/bookmarks/{bookmark_id}")
    def read(bookmark_id: int):
        with lock:
            return load(bookmark_id)

    @app.patch("/bookmarks/{bookmark_id}")
    def update(bookmark_id: int, patch: BookmarkPatch):
        given = patch.model_dump(exclude_unset=True)
        with lock:
            current = load(bookmark_id)
            url = current["url"]
            if "url" in given:
                if given["url"] is None:
                    raise HTTPException(status_code=422, detail="url cannot be null")
                host_of(given["url"])
                other = owner(given["url"])
                if other is not None and other != bookmark_id:
                    return JSONResponse(status_code=409, content={"detail": "duplicate url", "id": other})
                url = given["url"]
            title = current["title"]
            if "title" in given:
                title = given["title"].strip() if given["title"] and given["title"].strip() else host_of(url)
            db.execute("UPDATE bookmarks SET url = ?, title = ? WHERE id = ?", (url, title, bookmark_id))
            if "tags" in given:
                set_tags(bookmark_id, given["tags"] or [])
            db.commit()
            return load(bookmark_id)

    @app.delete("/bookmarks/{bookmark_id}", status_code=204)
    def delete(bookmark_id: int):
        with lock:
            load(bookmark_id)
            db.execute("DELETE FROM tags WHERE bookmark_id = ?", (bookmark_id,))
            db.execute("DELETE FROM bookmarks WHERE id = ?", (bookmark_id,))
            db.commit()
        return Response(status_code=204)

    @app.get("/tags")
    def tags():
        with lock:
            rows = db.execute("SELECT tag, count(*) AS n FROM tags GROUP BY tag ORDER BY n DESC, tag").fetchall()
        return [{"tag": tag, "count": n} for tag, n in rows]

    return app
