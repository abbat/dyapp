@testable import DYApp
import XCTest

final class SearchFiltersTests: XCTestCase {
    private func failure(_ inputs: SearchFilterInputs) -> String {
        do {
            _ = try inputs.conditions()
            return ""
        } catch {
            return searchErrorKey(error)
        }
    }

    func testOnlyOfferedFiltersBecomeConditions() throws {
        var inputs = SearchFilterInputs()
        inputs.ageMin = "30"
        inputs.heightMin = "170"
        inputs.country = "DE"
        inputs.place = 7
        XCTAssertTrue(try inputs.conditions().isEmpty)
        inputs.offered = [4, 20]
        XCTAssertEqual(try inputs.conditions().map(\.field), [4, 20])
        XCTAssertEqual(try inputs.conditions().map(\.strings), [["DE"], ["7"]])
    }

    func testBadOrReversedRangesNameTheirField() throws {
        var inputs = SearchFilterInputs()
        inputs.offered = [1_001, 46]
        XCTAssertEqual(try inputs.conditions().count, 0)
        inputs.ageMin = "old"
        XCTAssertEqual(failure(inputs), "search.ageInvalid")
        inputs.ageMin = "30"
        inputs.ageMax = "20"
        XCTAssertEqual(failure(inputs), "search.ageInvalid")
        inputs.ageMax = ""
        XCTAssertEqual(try inputs.conditions().count, 1)
        inputs.heightMin = "tall"
        XCTAssertEqual(failure(inputs), "search.heightInvalid")
        inputs.heightMin = "190"
        inputs.heightMax = "170"
        XCTAssertEqual(failure(inputs), "search.heightInvalid")
    }

    func testPickedScaleAppliesOnlyWhenOffered() throws {
        let scale = try XCTUnwrap(profileScales().first)
        let code = try XCTUnwrap(scale.choices.first?.code)
        var inputs = SearchFilterInputs()
        inputs.picked = [scale.key: code]
        XCTAssertTrue(try inputs.conditions().isEmpty)
        inputs.offered = [scale.field]
        XCTAssertEqual(try inputs.conditions().map(\.ints), [[Int64(code)]])
        inputs.picked = [scale.key: 0]
        XCTAssertTrue(try inputs.conditions().isEmpty)
    }

    func testMutualNeedsBothSeekingFieldsOffered() {
        var inputs = SearchFilterInputs()
        inputs.mutual = true
        inputs.offered = [53]
        XCTAssertFalse(inputs.mutualWanted)
        inputs.offered = [53, 54]
        XCTAssertTrue(inputs.mutualWanted)
    }

    func testEmployerSplitsCompaniesAndIgnoresABlankOne() {
        var inputs = SearchFilterInputs()
        inputs.company = " Acme ; ;Globex"
        XCTAssertEqual(
            String(describing: inputs.employer()), String(describing: SearchEmployer.any))
        inputs.employerKind = .same
        XCTAssertEqual(
            String(describing: inputs.employer()),
            String(describing: SearchEmployer.same(company: "Acme ; ;Globex")))
        inputs.employerKind = .exclude
        XCTAssertEqual(
            String(describing: inputs.employer()),
            String(describing: SearchEmployer.exclude(companies: ["Acme", "Globex"])))
        inputs.company = "  "
        XCTAssertEqual(
            String(describing: inputs.employer()), String(describing: SearchEmployer.any))
    }

    func testCoreErrorsMapToStatusTexts() {
        XCTAssertEqual(
            searchErrorKey(CoreError.Failed(message: "notConnected")), "search.notConnected")
        XCTAssertEqual(
            searchErrorKey(CoreError.Failed(message: "mutual unavailable")),
            "search.mutualUnavailable")
        XCTAssertEqual(searchErrorKey(CoreError.Failed(message: "other")), "search.failed")
    }

    func testRateLimitRoundsUpToWholeSeconds() {
        XCTAssertTrue(searchFailureText(.rateLimited(retryAfterMs: 1_001)).contains("2"))
    }
}
