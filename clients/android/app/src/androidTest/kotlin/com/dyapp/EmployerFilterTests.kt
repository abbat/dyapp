package com.dyapp

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.dyapp.SearchEmployer

class EmployerFilterTests {
    private fun employer(kind: EmployerKind, company: String) =
        EmployerFilter().apply {
            this.kind = kind
            this.company = company
        }.employer()

    @Test
    fun aBlankCompanyOrAnyLeavesEveryEmployer() {
        assertEquals(SearchEmployer.Any, employer(EmployerKind.SAME, "  "))
        assertEquals(SearchEmployer.Any, employer(EmployerKind.ANY, "Acme"))
    }

    @Test
    fun unicodeInputUsesTheLinuxCoreContractThroughNativeBindings() {
        assertEquals(SearchEmployer.Any, employer(EmployerKind.SAME, "\uFEFF\u00A0"))
        assertEquals(SearchEmployer.Same("\u0085"), employer(EmployerKind.SAME, "\u0085"))
        assertEquals(SearchEmployer.Exclude(emptyList()), employer(EmployerKind.EXCLUDE, "; ;"))
    }

    @Test
    fun sameKeepsOneCompanyAndExcludeSplitsOnSemicolons() {
        assertEquals(SearchEmployer.Same("Acme; Inc"), employer(EmployerKind.SAME, " Acme; Inc "))
        assertEquals(SearchEmployer.Exclude(listOf("Acme", "Globex")), employer(EmployerKind.EXCLUDE, "Acme; ;Globex;"))
    }
}
