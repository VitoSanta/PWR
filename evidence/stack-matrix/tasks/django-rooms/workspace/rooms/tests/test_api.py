import json

from django.test import TestCase


class Api(TestCase):
    def post(self, url, body):
        return self.client.post(url, json.dumps(body), content_type="application/json")

    def room(self, name="Blue", capacity=6):
        response = self.post("/api/rooms", {"name": name, "capacity": capacity})
        self.assertEqual(response.status_code, 201, response.content)
        return response.json()

    def book(self, room, start, end, attendees=2, title="Sync"):
        return self.post(f"/api/rooms/{room['id']}/bookings",
                         {"title": title, "start": start, "end": end, "attendees": attendees})

    def test_rooms(self):
        self.room("Orange", 4)
        blue = self.room("Blue", 8)
        self.assertEqual(blue["name"], "Blue")
        self.assertEqual(self.post("/api/rooms", {"name": "Blue", "capacity": 2}).status_code, 409)
        bad = self.post("/api/rooms", {"name": "", "capacity": 0})
        self.assertEqual(bad.status_code, 400)
        self.assertEqual(bad.json()["error"], "invalid")
        self.assertEqual(set(bad.json()["fields"]), {"name", "capacity"})
        self.assertEqual([r["name"] for r in self.client.get("/api/rooms").json()], ["Blue", "Orange"])
        self.assertEqual(self.client.post("/api/rooms", "[1]", content_type="application/json").status_code, 400)

    def test_booking_is_returned_in_utc(self):
        room = self.room()
        response = self.book(room, "2026-10-01T11:00:00+02:00", "2026-10-01T12:30:00+02:00")
        self.assertEqual(response.status_code, 201, response.content)
        body = response.json()
        self.assertEqual((body["room"], body["start"], body["end"]), (room["id"], "2026-10-01T09:00:00Z", "2026-10-01T10:30:00Z"))

    def test_validation(self):
        room = self.room(capacity=4)
        cases = [
            ("2026-10-01T10:00:00+00:00", "2026-10-01T09:00:00+00:00", 2),
            ("2026-10-01T09:00:00+00:00", "2026-10-01T17:15:00+00:00", 2),
            ("2026-10-01T09:10:00+00:00", "2026-10-01T10:00:00+00:00", 2),
            ("2026-10-01T09:00:00", "2026-10-01T10:00:00", 2),
            ("2026-10-01T09:00:00+00:00", "2026-10-01T10:00:00+00:00", 5),
            ("tomorrow", "2026-10-01T10:00:00+00:00", 2),
        ]
        for start, end, attendees in cases:
            response = self.book(room, start, end, attendees)
            self.assertEqual(response.status_code, 400, (start, end, attendees))
            self.assertEqual(response.json()["error"], "invalid")
        self.assertEqual(self.book(room, "2026-10-01T09:00:00+00:00", "2026-10-01T17:00:00+00:00").status_code, 201)
        self.assertEqual(self.post("/api/rooms/999/bookings", {}).status_code, 404)

    def test_overlaps(self):
        room = self.room()
        first = self.book(room, "2026-10-01T09:00:00+00:00", "2026-10-01T10:00:00+00:00").json()
        self.assertEqual(self.book(room, "2026-10-01T10:00:00+00:00", "2026-10-01T11:00:00+00:00").status_code, 201)
        clash = self.book(room, "2026-10-01T09:45:00+00:00", "2026-10-01T10:15:00+00:00")
        self.assertEqual(clash.status_code, 409)
        self.assertEqual(clash.json()["error"], "overlap")
        self.assertEqual(len(clash.json()["conflicts"]), 2)
        other = self.room("Green")
        self.assertEqual(self.book(other, "2026-10-01T09:00:00+00:00", "2026-10-01T10:00:00+00:00").status_code, 201)
        self.assertEqual(self.client.delete(f"/api/bookings/{first['id']}").status_code, 204)
        self.assertEqual(self.client.delete(f"/api/bookings/{first['id']}").status_code, 404)

    def test_listing_by_day(self):
        room = self.room()
        self.book(room, "2026-10-02T09:00:00+00:00", "2026-10-02T10:00:00+00:00", title="b")
        self.book(room, "2026-10-01T23:00:00+00:00", "2026-10-02T01:00:00+00:00", title="a")
        self.book(room, "2026-10-03T09:00:00+00:00", "2026-10-03T10:00:00+00:00", title="c")
        titles = [b["title"] for b in self.client.get(f"/api/rooms/{room['id']}/bookings").json()]
        self.assertEqual(titles, ["a", "b", "c"])
        day = [b["title"] for b in self.client.get(f"/api/rooms/{room['id']}/bookings?date=2026-10-02").json()]
        self.assertEqual(day, ["a", "b"])

    def test_availability(self):
        small, big = self.room("Small", 2), self.room("Big", 10)
        self.room("Busy", 10)
        busy = self.client.get("/api/rooms").json()[1]
        self.book(busy, "2026-10-01T09:00:00+00:00", "2026-10-01T11:00:00+00:00")
        query = "start=2026-10-01T10:00:00%2B00:00&end=2026-10-01T10:30:00%2B00:00&attendees=4"
        names = [r["name"] for r in self.client.get(f"/api/rooms/available?{query}").json()]
        self.assertEqual(names, ["Big"])
        self.assertEqual(self.client.get("/api/rooms/available?start=x&end=y&attendees=1").status_code, 400)
        self.assertTrue(small and big)
