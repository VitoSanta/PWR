package com.example.library;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;

@Entity
public class Book {
    @Id
    private String isbn;
    private String title;
    private String author;
    private int copies;
    private int available;

    protected Book() {}

    Book(String isbn, String title, String author, int copies) {
        this.isbn = isbn;
        this.title = title;
        this.author = author;
        this.copies = copies;
        this.available = copies;
    }

    public String getIsbn() { return isbn; }
    public String getTitle() { return title; }
    public String getAuthor() { return author; }
    public int getCopies() { return copies; }
    public int getAvailable() { return available; }
    void take() { available--; }
    void giveBack() { available++; }
}
