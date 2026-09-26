package com.example.library;

import jakarta.validation.Valid;
import jakarta.validation.constraints.Max;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Pattern;
import java.time.Clock;
import java.time.LocalDate;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import org.springframework.http.HttpStatus;
import org.springframework.transaction.annotation.Transactional;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/api")
class LibraryController {
    record BookRequest(
        @NotNull @Pattern(regexp = "\\d{13}", message = "must be 13 digits") String isbn,
        @NotBlank String title,
        @NotBlank String author,
        @NotNull @Min(1) @Max(100) Integer copies) {}

    record LoanRequest(@NotBlank String isbn, @NotBlank String member) {}

    private final BookRepository books;
    private final LoanRepository loans;
    private final Clock clock;

    LibraryController(BookRepository books, LoanRepository loans, Clock clock) {
        this.books = books;
        this.loans = loans;
        this.clock = clock;
    }

    private LocalDate today() {
        return LocalDate.now(clock);
    }

    @PostMapping("/books")
    @ResponseStatus(HttpStatus.CREATED)
    @Transactional
    Book addBook(@Valid @RequestBody BookRequest request) {
        if (books.existsById(request.isbn())) {
            throw new ApiException(HttpStatus.CONFLICT, "duplicate isbn");
        }
        return books.save(new Book(request.isbn(), request.title().trim(), request.author().trim(), request.copies()));
    }

    @GetMapping("/books")
    List<Book> listBooks(@RequestParam(required = false) String author) {
        return author == null ? books.findAllByOrderByTitleAsc() : books.findByAuthorContainingIgnoreCaseOrderByTitleAsc(author);
    }

    @GetMapping("/books/{isbn}")
    Book book(@PathVariable String isbn) {
        return books.findById(isbn).orElseThrow(() -> new ApiException(HttpStatus.NOT_FOUND, "book not found"));
    }

    @PostMapping("/loans")
    @ResponseStatus(HttpStatus.CREATED)
    @Transactional
    Loan lend(@Valid @RequestBody LoanRequest request) {
        Book book = book(request.isbn());
        String member = request.member().trim();
        List<Loan> active = loans.findByMemberAndReturnedOnIsNullOrderByDueOnAscIdAsc(member);
        if (active.stream().anyMatch(l -> l.getDueOn().isBefore(today()))) {
            throw new ApiException(HttpStatus.CONFLICT, "overdue loans");
        }
        if (active.size() >= 3) {
            throw new ApiException(HttpStatus.CONFLICT, "loan limit reached");
        }
        if (book.getAvailable() <= 0) {
            throw new ApiException(HttpStatus.CONFLICT, "no copies available");
        }
        book.take();
        return loans.save(new Loan(book.getIsbn(), member, today()));
    }

    @PostMapping("/loans/{id}/return")
    @Transactional
    Loan giveBack(@PathVariable long id) {
        Loan loan = loans.findById(id).orElseThrow(() -> new ApiException(HttpStatus.NOT_FOUND, "loan not found"));
        if (loan.getReturnedOn() != null) {
            throw new ApiException(HttpStatus.CONFLICT, "already returned");
        }
        loan.markReturned(today());
        books.findById(loan.getIsbn()).ifPresent(Book::giveBack);
        return loan;
    }

    @GetMapping("/members/{member}/loans")
    List<Map<String, Object>> memberLoans(@PathVariable String member) {
        LocalDate today = today();
        return loans.findByMemberAndReturnedOnIsNullOrderByDueOnAscIdAsc(member).stream().map(loan -> {
            Map<String, Object> view = new LinkedHashMap<>();
            view.put("id", loan.getId());
            view.put("isbn", loan.getIsbn());
            view.put("member", loan.getMember());
            view.put("loanedOn", loan.getLoanedOn());
            view.put("dueOn", loan.getDueOn());
            view.put("returnedOn", null);
            view.put("overdue", loan.getDueOn().isBefore(today));
            return view;
        }).toList();
    }
}
