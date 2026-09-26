package com.example.library;

import jakarta.persistence.Entity;
import jakarta.persistence.GeneratedValue;
import jakarta.persistence.Id;
import java.time.LocalDate;

@Entity
public class Loan {
    @Id
    @GeneratedValue
    private Long id;
    private String isbn;
    private String member;
    private LocalDate loanedOn;
    private LocalDate dueOn;
    private LocalDate returnedOn;

    protected Loan() {}

    Loan(String isbn, String member, LocalDate today) {
        this.isbn = isbn;
        this.member = member;
        this.loanedOn = today;
        this.dueOn = today.plusDays(14);
    }

    public Long getId() { return id; }
    public String getIsbn() { return isbn; }
    public String getMember() { return member; }
    public LocalDate getLoanedOn() { return loanedOn; }
    public LocalDate getDueOn() { return dueOn; }
    public LocalDate getReturnedOn() { return returnedOn; }
    void markReturned(LocalDate today) { this.returnedOn = today; }
}
