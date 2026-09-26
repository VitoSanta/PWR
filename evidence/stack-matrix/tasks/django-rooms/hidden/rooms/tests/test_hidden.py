import json

from django.test import TestCase

from rooms.models import Booking, Room


class Hidden(TestCase):
    def post(self, url, body):
        return self.client.post(url, json.dumps(body), content_type="application/json")

    def test_deleting_a_room_deletes_its_bookings(self):
        room = self.post("/api/rooms", {"name": "A", "capacity": 3}).json()
        self.post(f"/api/rooms/{room['id']}/bookings", {"title": "x", "start": "2026-10-01T09:00:00+00:00", "end": "2026-10-01T09:15:00+00:00", "attendees": 1})
        Room.objects.get(pk=room["id"]).delete()
        self.assertEqual(Booking.objects.count(), 0)

    def test_every_invalid_field_is_named(self):
        room = self.post("/api/rooms", {"name": "A", "capacity": 3}).json()
        response = self.post(f"/api/rooms/{room['id']}/bookings", {"title": "", "start": "nope", "attendees": 0})
        self.assertEqual(response.status_code, 400)
        self.assertTrue({"title", "start", "end", "attendees"} <= set(response.json()["fields"]))

    def test_seconds_break_the_quarter_hour(self):
        room = self.post("/api/rooms", {"name": "A", "capacity": 3}).json()
        response = self.post(f"/api/rooms/{room['id']}/bookings", {"title": "x", "start": "2026-10-01T09:00:30+00:00", "end": "2026-10-01T10:00:00+00:00", "attendees": 1})
        self.assertEqual(response.status_code, 400)

    def test_availability_counts_touching_as_free(self):
        room = self.post("/api/rooms", {"name": "A", "capacity": 3}).json()
        self.post(f"/api/rooms/{room['id']}/bookings", {"title": "x", "start": "2026-10-01T09:00:00+00:00", "end": "2026-10-01T10:00:00+00:00", "attendees": 1})
        query = "start=2026-10-01T10:00:00Z&end=2026-10-01T11:00:00Z&attendees=3"
        self.assertEqual([r["name"] for r in self.client.get(f"/api/rooms/available?{query}").json()], ["A"])

    def test_names_are_unique_in_the_database_too(self):
        Room.objects.create(name="Solo", capacity=1)
        from django.db import IntegrityError, transaction
        with self.assertRaises(IntegrityError), transaction.atomic():
            Room.objects.create(name="Solo", capacity=2)

    def test_migrations_are_complete(self):
        from io import StringIO
        from django.core.management import call_command
        out = StringIO()
        call_command("makemigrations", "rooms", "--check", "--dry-run", stdout=out)
