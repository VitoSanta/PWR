package com.example.library;

import java.util.List;
import org.springframework.data.jpa.repository.JpaRepository;

interface BookRepository extends JpaRepository<Book, String> {
    List<Book> findAllByOrderByTitleAsc();

    List<Book> findByAuthorContainingIgnoreCaseOrderByTitleAsc(String author);
}

interface LoanRepository extends JpaRepository<Loan, Long> {
    List<Loan> findByMemberAndReturnedOnIsNullOrderByDueOnAscIdAsc(String member);
}
