from django.db import models


class Room(models.Model):
    name = models.CharField(max_length=60, unique=True)
    capacity = models.PositiveIntegerField()

    class Meta:
        ordering = ["name"]


class Booking(models.Model):
    room = models.ForeignKey(Room, on_delete=models.CASCADE, related_name="bookings")
    title = models.CharField(max_length=120)
    start = models.DateTimeField()
    end = models.DateTimeField()
    attendees = models.PositiveIntegerField()

    class Meta:
        ordering = ["start", "id"]
