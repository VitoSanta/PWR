package com.example.library;

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import java.time.Clock;
import java.time.Instant;
import java.time.ZoneOffset;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.context.annotation.Primary;
import org.springframework.http.MediaType;
import org.springframework.test.annotation.DirtiesContext;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.ResultActions;

@SpringBootTest
@AutoConfigureMockMvc
@DirtiesContext(classMode = DirtiesContext.ClassMode.AFTER_EACH_TEST_METHOD)
@Import(LibraryApiTest.FixedClock.class)
class LibraryApiTest {

    @TestConfiguration
    static class FixedClock {
        @Bean
        @Primary
        Clock fixedClock() {
            return Clock.fixed(Instant.parse("2026-10-01T10:00:00Z"), ZoneOffset.UTC);
        }
    }

    @Autowired MockMvc mvc;

    ResultActions send(String url, String json) throws Exception {
        return mvc.perform(post(url).contentType(MediaType.APPLICATION_JSON).content(json));
    }

    void book(String isbn, String title, String author, int copies) throws Exception {
        send("/api/books", "{\"isbn\":\"%s\",\"title\":\"%s\",\"author\":\"%s\",\"copies\":%d}".formatted(isbn, title, author, copies))
            .andExpect(status().isCreated());
    }

    ResultActions lend(String isbn, String member) throws Exception {
        return send("/api/loans", "{\"isbn\":\"%s\",\"member\":\"%s\"}".formatted(isbn, member));
    }

    @Test
    void createsAndListsBooks() throws Exception {
        book("9780000000002", "Zen", "Robert Pirsig", 1);
        send("/api/books", "{\"isbn\":\"9780000000001\",\"title\":\"Dune\",\"author\":\"Frank Herbert\",\"copies\":2}")
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.available").value(2));
        mvc.perform(get("/api/books")).andExpect(jsonPath("$[0].title").value("Dune")).andExpect(jsonPath("$[1].title").value("Zen"));
        mvc.perform(get("/api/books?author=HERB")).andExpect(jsonPath("$.length()").value(1));
        mvc.perform(get("/api/books/9780000000001")).andExpect(status().isOk()).andExpect(jsonPath("$.author").value("Frank Herbert"));
        mvc.perform(get("/api/books/9789999999999")).andExpect(status().isNotFound());
    }

    @Test
    void validatesBooks() throws Exception {
        send("/api/books", "{\"isbn\":\"123\",\"title\":\" \",\"author\":\"A\",\"copies\":0}")
            .andExpect(status().isBadRequest())
            .andExpect(jsonPath("$.errors.isbn").exists())
            .andExpect(jsonPath("$.errors.title").exists())
            .andExpect(jsonPath("$.errors.copies").exists())
            .andExpect(jsonPath("$.errors.author").doesNotExist());
        book("9780000000001", "Dune", "Frank Herbert", 1);
        send("/api/books", "{\"isbn\":\"9780000000001\",\"title\":\"Again\",\"author\":\"X\",\"copies\":1}")
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.error").value("duplicate isbn"));
    }

    @Test
    void lendsAndReturns() throws Exception {
        book("9780000000001", "Dune", "Frank Herbert", 1);
        String body = lend("9780000000001", "ada")
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.loanedOn").value("2026-10-01"))
            .andExpect(jsonPath("$.dueOn").value("2026-10-15"))
            .andExpect(jsonPath("$.returnedOn").isEmpty())
            .andReturn().getResponse().getContentAsString();
        long id = Long.parseLong(body.replaceAll(".*\"id\":(\\d+).*", "$1"));
        mvc.perform(get("/api/books/9780000000001")).andExpect(jsonPath("$.available").value(0));
        lend("9780000000001", "bob").andExpect(status().isConflict()).andExpect(jsonPath("$.error").value("no copies available"));
        mvc.perform(post("/api/loans/" + id + "/return"))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.returnedOn").value("2026-10-01"));
        mvc.perform(post("/api/loans/" + id + "/return")).andExpect(status().isConflict());
        mvc.perform(post("/api/loans/999/return")).andExpect(status().isNotFound());
        lend("9780000000001", "bob").andExpect(status().isCreated());
        lend("9789999999999", "bob").andExpect(status().isNotFound());
        lend("9780000000001", " ").andExpect(status().isBadRequest()).andExpect(jsonPath("$.errors.member").exists());
    }

    @Test
    void limitsLoansPerMember() throws Exception {
        book("9780000000001", "Dune", "Frank Herbert", 10);
        for (int i = 0; i < 3; i++) {
            lend("9780000000001", "ada").andExpect(status().isCreated());
        }
        lend("9780000000001", "ada").andExpect(status().isConflict()).andExpect(jsonPath("$.error").value("loan limit reached"));
        mvc.perform(get("/api/members/ada/loans"))
            .andExpect(jsonPath("$.length()").value(3))
            .andExpect(jsonPath("$[0].overdue").value(false));
    }
}
