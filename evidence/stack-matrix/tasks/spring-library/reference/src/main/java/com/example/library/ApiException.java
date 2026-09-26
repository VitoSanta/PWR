package com.example.library;

import org.springframework.http.HttpStatus;

class ApiException extends RuntimeException {
    final HttpStatus status;

    ApiException(HttpStatus status, String message) {
        super(message);
        this.status = status;
    }
}
