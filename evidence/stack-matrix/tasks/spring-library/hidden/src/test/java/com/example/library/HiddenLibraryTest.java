package com.example.library;

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import java.time.Clock;
import java.time.Instant;
import java.time.ZoneOffset;
import java.util.concurrent.atomic.AtomicReference;
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

@SpringBootTest
@AutoConfigureMockMvc
@DirtiesContext(classMode = DirtiesContext.ClassMode.AFTER_EACH_TEST_METHOD)
@Import(HiddenLibraryTest.MovableClock.class)
class HiddenLibraryTest {
    static final AtomicReference<Instant> NOW = new AtomicReference<>(Instant.parse("2026-10-01T10:00:00Z"));

    @TestConfiguration
    static class MovableClock {
        @Bean
        @Primary
        Clock movableClock() {
            return new Clock() {
                public java.time.ZoneId getZone() { return ZoneOffset.UTC; }
                public Clock withZone(java.time.ZoneId zone) { return this; }
                public Instant instant() { return NOW.get(); }
            };
        }
    }

    @Autowired MockMvc mvc;

    @Test
    void overdueLoansBlockBorrowingAndAreFlagged() throws Exception {
        NOW.set(Instant.parse("2026-10-01T10:00:00Z"));
        mvc.perform(post("/api/books").contentType(MediaType.APPLICATION_JSON)
            .content("{\"isbn\":\"9780000000001\",\"title\":\"Dune\",\"author\":\"F\",\"copies\":5}")).andExpect(status().isCreated());
        mvc.perform(post("/api/loans").contentType(MediaType.APPLICATION_JSON).content("{\"isbn\":\"9780000000001\",\"member\":\"late\"}"))
            .andExpect(status().isCreated());
        NOW.set(Instant.parse("2026-10-15T23:59:00Z"));
        mvc.perform(get("/api/members/late/loans")).andExpect(jsonPath("$[0].overdue").value(false));
        NOW.set(Instant.parse("2026-10-16T00:00:00Z"));
        mvc.perform(get("/api/members/late/loans")).andExpect(jsonPath("$[0].overdue").value(true));
        mvc.perform(post("/api/loans").contentType(MediaType.APPLICATION_JSON).content("{\"isbn\":\"9780000000001\",\"member\":\"late\"}"))
            .andExpect(status().isConflict()).andExpect(jsonPath("$.error").value("overdue loans"));
        mvc.perform(post("/api/loans").contentType(MediaType.APPLICATION_JSON).content("{\"isbn\":\"9780000000001\",\"member\":\"prompt\"}"))
            .andExpect(status().isCreated()).andExpect(jsonPath("$.dueOn").value("2026-10-30"));
    }

    @Test
    void copiesAreBoundedAndReturnsFreeASlot() throws Exception {
        NOW.set(Instant.parse("2026-10-01T10:00:00Z"));
        mvc.perform(post("/api/books").contentType(MediaType.APPLICATION_JSON)
            .content("{\"isbn\":\"9780000000009\",\"title\":\"T\",\"author\":\"A\",\"copies\":101}")).andExpect(status().isBadRequest());
        mvc.perform(post("/api/books").contentType(MediaType.APPLICATION_JSON)
            .content("{\"isbn\":\"97800000000AB\",\"title\":\"T\",\"author\":\"A\",\"copies\":1}")).andExpect(status().isBadRequest());
        mvc.perform(post("/api/books").contentType(MediaType.APPLICATION_JSON)
            .content("{\"isbn\":\"9780000000009\",\"title\":\"T\",\"author\":\"A\",\"copies\":4}")).andExpect(status().isCreated());
        String first = mvc.perform(post("/api/loans").contentType(MediaType.APPLICATION_JSON).content("{\"isbn\":\"9780000000009\",\"member\":\"m\"}"))
            .andReturn().getResponse().getContentAsString();
        for (int i = 0; i < 2; i++) {
            mvc.perform(post("/api/loans").contentType(MediaType.APPLICATION_JSON).content("{\"isbn\":\"9780000000009\",\"member\":\"m\"}"));
        }
        long id = Long.parseLong(first.replaceAll(".*\"id\":(\\d+).*", "$1"));
        mvc.perform(post("/api/loans/" + id + "/return")).andExpect(status().isOk());
        mvc.perform(post("/api/loans").contentType(MediaType.APPLICATION_JSON).content("{\"isbn\":\"9780000000009\",\"member\":\"m\"}"))
            .andExpect(status().isCreated());
        mvc.perform(get("/api/books/9780000000009")).andExpect(jsonPath("$.available").value(1));
        mvc.perform(get("/api/members/m/loans")).andExpect(jsonPath("$.length()").value(3));
    }
}
