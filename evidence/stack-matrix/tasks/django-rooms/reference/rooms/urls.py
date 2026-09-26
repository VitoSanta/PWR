from django.urls import path

from . import views

urlpatterns = [
    path("rooms", views.rooms),
    path("rooms/available", views.available),
    path("rooms/<int:room_id>/bookings", views.room_bookings),
    path("bookings/<int:booking_id>", views.booking),
]
