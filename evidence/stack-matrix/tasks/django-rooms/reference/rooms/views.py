import json
from datetime import date, datetime, timedelta, timezone

from django.http import HttpResponse, JsonResponse
from django.views.decorators.csrf import csrf_exempt

from .models import Booking, Room


def error(status, message, **extra):
    return JsonResponse({"error": message, **extra}, status=status)


def invalid(fields):
    return error(400, "invalid", fields=fields)


def body_of(request):
    try:
        body = json.loads(request.body or b"null")
    except ValueError:
        return None
    return body if isinstance(body, dict) else None


def moment(text):
    if not isinstance(text, str):
        return None
    try:
        value = datetime.fromisoformat(text)
    except ValueError:
        return None
    return value if value.tzinfo is not None else None


def utc(value):
    return value.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def positive(value):
    return isinstance(value, int) and not isinstance(value, bool) and value >= 1


def room_json(room):
    return {"id": room.id, "name": room.name, "capacity": room.capacity}


def booking_json(booking):
    return {"id": booking.id, "room": booking.room_id, "title": booking.title,
            "start": utc(booking.start), "end": utc(booking.end), "attendees": booking.attendees}


@csrf_exempt
def rooms(request):
    if request.method == "GET":
        return JsonResponse([room_json(r) for r in Room.objects.order_by("name")], safe=False)
    if request.method != "POST":
        return error(405, "method not allowed")
    body = body_of(request)
    if body is None:
        return error(400, "body must be a JSON object")
    fields = {}
    name = body.get("name")
    if not isinstance(name, str) or not 1 <= len(name.strip()) <= 60:
        fields["name"] = "1-60 characters"
    if not positive(body.get("capacity")):
        fields["capacity"] = "a positive integer"
    if fields:
        return invalid(fields)
    if Room.objects.filter(name=name.strip()).exists():
        return error(409, "duplicate name")
    return JsonResponse(room_json(Room.objects.create(name=name.strip(), capacity=body["capacity"])), status=201)


def check_interval(start, end, fields):
    if start is None:
        fields["start"] = "an ISO 8601 datetime with an offset"
    if end is None:
        fields["end"] = "an ISO 8601 datetime with an offset"
    if start is None or end is None:
        return
    if start >= end:
        fields["end"] = "must be after start"
    elif end - start > timedelta(hours=8):
        fields["end"] = "at most 8 hours after start"
    for name, value in (("start", start), ("end", end)):
        if value.minute % 15 or value.second or value.microsecond:
            fields.setdefault(name, "on a quarter hour")


@csrf_exempt
def room_bookings(request, room_id):
    room = Room.objects.filter(pk=room_id).first()
    if room is None:
        return error(404, "room not found")
    if request.method == "GET":
        bookings = room.bookings.order_by("start", "id")
        day = request.GET.get("date")
        if day:
            try:
                first = datetime.combine(date.fromisoformat(day), datetime.min.time(), tzinfo=timezone.utc)
            except ValueError:
                return invalid({"date": "YYYY-MM-DD"})
            bookings = bookings.filter(start__lt=first + timedelta(days=1), end__gt=first)
        return JsonResponse([booking_json(b) for b in bookings], safe=False)
    if request.method != "POST":
        return error(405, "method not allowed")
    body = body_of(request)
    if body is None:
        return error(400, "body must be a JSON object")
    fields = {}
    title = body.get("title")
    if not isinstance(title, str) or not 1 <= len(title.strip()) <= 120:
        fields["title"] = "1-120 characters"
    start, end = moment(body.get("start")), moment(body.get("end"))
    check_interval(start, end, fields)
    attendees = body.get("attendees")
    if not positive(attendees):
        fields["attendees"] = "a positive integer"
    elif attendees > room.capacity:
        fields["attendees"] = f"the room holds {room.capacity}"
    if fields:
        return invalid(fields)
    conflicts = list(room.bookings.filter(start__lt=end, end__gt=start).order_by("start").values_list("id", flat=True))
    if conflicts:
        return JsonResponse({"error": "overlap", "conflicts": conflicts}, status=409)
    booking = Booking.objects.create(room=room, title=title.strip(), start=start, end=end, attendees=attendees)
    return JsonResponse(booking_json(booking), status=201)


@csrf_exempt
def booking(request, booking_id):
    if request.method != "DELETE":
        return error(405, "method not allowed")
    deleted, _ = Booking.objects.filter(pk=booking_id).delete()
    return HttpResponse(status=204) if deleted else error(404, "booking not found")


def available(request):
    fields = {}
    start, end = moment(request.GET.get("start")), moment(request.GET.get("end"))
    check_interval(start, end, fields)
    try:
        attendees = int(request.GET.get("attendees", ""))
    except ValueError:
        attendees = 0
    if attendees < 1:
        fields["attendees"] = "a positive integer"
    if fields:
        return invalid(fields)
    busy = Booking.objects.filter(start__lt=end, end__gt=start).values("room_id")
    free = Room.objects.filter(capacity__gte=attendees).exclude(id__in=busy).order_by("name")
    return JsonResponse([room_json(r) for r in free], safe=False)
