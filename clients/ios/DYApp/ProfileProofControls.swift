import SwiftUI

private enum ProfilePublishAttempt: Sendable {
    case published(PublishedProfileSnapshot)
    case failed(String)
}

private func publicationErrorKey(_ error: Error) -> String {
    let message = String(describing: error)
    if message.contains("blocked: underage") { return "profile.publishError.underage" }
    if message.contains("blocked: notAttested") { return "profile.publishError.notAttested" }
    if message.contains("blocked: invalidBirthDate") {
        return "profile.publishError.invalidBirthDate"
    }
    if message.contains("notConnected") { return "profile.publishError.notConnected" }
    if message.contains("unsolved") { return "profile.publishError.unsolved" }
    if message.contains("readback differs") { return "profile.publishError.readback" }
    return "profile.publishError.generic"
}

/// Hides the profile from search now; only a new publication shows it again. Before a first
/// publication, or with an unreadable state, the action stays disabled.
private struct ProfileHideControls: View {
    let candidate: IdentityCandidate
    let peerId: String
    let publishedVersion: UInt64?
    let disabled: Bool
    let onHidden: @MainActor () -> Void

    @State private var visibility: ProfileVisibility?
    @State private var isHiding = false
    @State private var errorKey: String?
    @State private var opening = UUID()
    @State private var review: SearchWithdrawalReview?
    @State private var recovered: SearchRecoverySaved?
    @State private var recoveryError: String?
    @State private var recovering = false

    var body: some View {
        VStack(alignment: .leading) {
            if review != nil || recovered != nil || recoveryError != nil {
                recoveryControls
            } else if visibility?.hidden == true {
                Text("profile.hiddenNotice")
                    .accessibilityIdentifier("profile-hidden-notice")
            } else {
                Button("profile.hide") {
                    let expected = opening
                    Task { await hide(in: expected) }
                }
                    .disabled(disabled || isHiding || recovering || (visibility?.version ?? 0) == 0)
                    .accessibilityIdentifier("profile-hide")
                Text("profile.hideHint")
                    .font(.caption)
            }
            if isHiding || recovering { ProgressView() }
            if let errorKey {
                Text(LocalizedStringKey(errorKey))
                    .accessibilityIdentifier("profile-hide-error")
            }
        }
        .task(id: publishedVersion) { await load() }
        .onDisappear { closeRecovery() }
    }

    private var recoveryControls: some View {
        VStack(alignment: .leading) {
            Text("searchRecovery.title")
            Text(verbatim: peerId)
                .textSelection(.enabled)
                .accessibilityIdentifier("search-recovery-owner")
            Text(LocalizedStringKey(recoveryError ?? recoveryStatus))
                .accessibilityIdentifier("search-recovery-status")
            if recovered?.searchPending != false {
                Button(LocalizedStringKey(recoveryAction)) {
                    let expected = opening
                    Task { await advanceRecovery(in: expected) }
                }
                .disabled(disabled || isHiding || recovering)
                .accessibilityIdentifier("search-recovery-action")
            }
            Button("searchRecovery.close") { closeRecovery() }
                .disabled(recovering)
                .accessibilityIdentifier("search-recovery-close")
        }
    }

    private var recoveryStatus: String {
        guard let recovered else { return "searchRecovery.review" }
        return recovered.searchPending ? "searchRecovery.pending" : "searchRecovery.sent"
    }

    private var recoveryAction: String {
        if canRetryRecovery { return "searchRecovery.retry" }
        return recoveryError == nil ? "searchRecovery.confirm" : "searchRecovery.refresh"
    }

    private var canRetryRecovery: Bool {
        recovered?.searchPending == true
            && (recoveryError == nil || recoveryError == "searchRecovery.error")
    }

    @MainActor
    private func load() async {
        let expected = opening
        let selected = candidate
        let selectedPeer = peerId
        let read = await IdentityIO.performProfile {
            try? RustBridge.visibility(for: selected, peerId: selectedPeer)
        }
        guard opening == expected, !Task.isCancelled else { return }
        visibility = read
        if review == nil && recovered == nil && recoveryError == nil {
            await prepareRecovery(in: expected)
        }
    }

    @MainActor
    private func prepareRecovery(in expected: UUID) async {
        guard opening == expected, !isHiding, !recovering, !Task.isCancelled else { return }
        recovering = true
        let selected = candidate
        let selectedPeer = peerId
        let result = await IdentityIO.performProfile {
            Result { try RustBridge.searchRecovery(for: selected, peerId: selectedPeer).prepare() }
        }
        guard opening == expected else { return }
        recovering = false
        guard !Task.isCancelled else { return }
        switch result {
        case .success(let prepared):
            retireReview()
            review = prepared
            recoveryError = nil

        case .failure(let error):
            recoveryError = recoveryErrorKey(error)
        }
    }

    @MainActor
    private func advanceRecovery(in expected: UUID) async {
        guard opening == expected, !disabled, !isHiding, !recovering else { return }
        let retry = canRetryRecovery
        guard let review, retry || recoveryError == nil else {
            recovered = nil
            await prepareRecovery(in: expected)
            return
        }
        recovering = true
        let result = await IdentityIO.performProfile {
            Result { retry ? try review.retry() : try review.confirm() }
        }
        guard opening == expected else { return }
        recovering = false
        switch result {
        case .success(let saved):
            recovered = saved
            recoveryError = nil
            visibility = ProfileVisibility(version: saved.version, hidden: saved.hidden)

        case .failure(let error):
            recoveryError = recoveryErrorKey(error)
        }
    }

    @MainActor
    private func closeRecovery() {
        opening = UUID()
        isHiding = false
        recovering = false
        recovered = nil
        recoveryError = nil
        retireReview()
    }

    @MainActor
    private func retireReview() {
        guard let oldReview = review else { return }
        review = nil
        IdentityIO.enqueueProfile { _ = try? oldReview.dismiss() }
    }

    private func recoveryErrorKey(_ error: Error) -> String {
        switch error as? SearchRecoveryError {
        case .Closed, .Changed:
            return "searchRecovery.stale"

        case .ReviewRequired:
            return "searchRecovery.reviewRequired"

        case .LinkPending:
            return "searchRecovery.linkPending"

        case .VersionExhausted:
            return "searchRecovery.versionExhausted"

        case .WrongOwner:
            return "searchRecovery.wrongOwner"

        default:
            return "searchRecovery.error"
        }
    }

    @MainActor
    private func hide(in expected: UUID) async {
        guard opening == expected, !disabled, !isHiding, !recovering, review == nil,
            recovered == nil, recoveryError == nil, (visibility?.version ?? 0) > 0
        else { return }
        isHiding = true
        errorKey = nil
        let selected = candidate
        let selectedPeer = peerId
        let outcome = await IdentityIO.performProfile {
            () -> Result<ProfileVisibility, Error> in
            Result { try RustBridge.hideProfile(for: selected, peerId: selectedPeer) }
        }
        guard opening == expected else { return }
        isHiding = false
        switch outcome {
        case .success(let hidden):
            visibility = hidden
            onHidden()

        case .failure(let error):
            errorKey = publicationErrorKey(error)
        }
    }
}

/// Prepare the saved profile proof in cancellable, persisted Rust steps.
struct ProfileProofControls: View {
    let candidate: IdentityCandidate
    let peerId: String
    let canPrepare: @MainActor () -> Bool
    let attested: Bool
    let resetToken: Int
    @Binding var isSolving: Bool
    let clearConfirmations: @MainActor () -> Void

    @State private var session: ProfileSolveSession?
    @State private var cancelRequested = false
    @State private var proofReady = false
    @State private var proofCancelled = false
    @State private var proofFailed = false
    @State private var nextNonce: UInt64 = 0
    @State private var isPublishing = false
    @State private var opening = UUID()
    @State private var publishErrorKey: String?
    @State private var published: PublishedProfileSnapshot?
    @State private var publishedPreview: ProfilePublicPreview?

    var body: some View {
        Group {
            Button(
                LocalizedStringKey(
                    publishErrorKey != nil
                        ? "profile.retryPublish"
                        : proofCancelled || proofFailed ? "profile.retryProof" : "profile.publish")
            ) {
                let expectedOpening = opening
                Task { await prepareProof(in: expectedOpening) }
            }
            .disabled(!canPrepare())
            .accessibilityIdentifier("profile-prepare-publish")
            if isSolving && !isPublishing {
                ProgressView()
                Button("profile.cancelProof") { cancelProof() }
                    .disabled(cancelRequested)
                    .accessibilityIdentifier("profile-cancel-proof")
            }
            if isPublishing {
                ProgressView()
                Text("profile.publishing")
                    .accessibilityIdentifier("profile-publish-status")
            }
            if nextNonce > 0 {
                HStack {
                    Text("profile.proofNonce")
                    Spacer()
                    Text(verbatim: String(nextNonce))
                }
                .accessibilityIdentifier("profile-proof-progress")
            }
            if proofReady && published == nil { Text("profile.proofReady") }
            if proofCancelled { Text("profile.proofCancelled") }
            if proofFailed { Text("profile.proofFailed") }
            if let publishErrorKey {
                Text(LocalizedStringKey(publishErrorKey))
                    .accessibilityIdentifier("profile-publish-error")
            }
            if let published, let publishedPreview {
                publishedReadback(published, preview: publishedPreview)
            }
            ProfileHideControls(
                candidate: candidate, peerId: peerId, publishedVersion: published?.version,
                disabled: isSolving || isPublishing, onHidden: resetProofState)
                .id(candidate.location.marker + ":" + peerId)
        }
        .onChange(of: resetToken) { _ in
            cancelProof()
            resetProofState()
        }
        .onDisappear {
            opening = UUID()
            cancelProof()
            session = nil
            isPublishing = false
            isSolving = false
        }
    }

    @MainActor
    private func resetProofState() {
        proofReady = false
        proofCancelled = false
        proofFailed = false
        nextNonce = 0
        publishErrorKey = nil
        published = nil
        publishedPreview = nil
    }

    @MainActor
    private func prepareProof(in expectedOpening: UUID) async {
        guard opening == expectedOpening, canPrepare(), !isSolving, !isPublishing else { return }
        let confirmedAdult = attested
        clearConfirmations()
        isSolving = true
        cancelRequested = false
        resetProofState()
        let selected = candidate
        let selectedPeer = peerId
        let opened = await IdentityIO.performBlocking {
            try? RustBridge.profileSolveSession(for: selected, peerId: selectedPeer)
        }
        guard opening == expectedOpening else {
            opened?.cancel()
            return
        }
        guard !Task.isCancelled else {
            opened?.cancel()
            isSolving = false
            return
        }
        guard let opened else {
            isSolving = false
            proofFailed = true
            return
        }
        if cancelRequested {
            opened.cancel()
            isSolving = false
            proofCancelled = true
            return
        }
        session = opened
        await runSteps(opened, attested: confirmedAdult, in: expectedOpening)
    }

    @MainActor
    private func runSteps(
        _ opened: ProfileSolveSession, attested: Bool, in expectedOpening: UUID
    ) async {
        while true {
            let result = await opened.runStep(attested: attested)
            guard opening == expectedOpening else {
                opened.cancel()
                return
            }
            guard !Task.isCancelled else {
                opened.cancel()
                session = nil
                isSolving = false
                return
            }
            switch result {
            case .failed:
                session = nil
                isSolving = false
                proofFailed = true
                return

            case .progress(let progress):
                nextNonce = progress.nextNonce
                if progress.cancelled || cancelRequested {
                    session = nil
                    isSolving = false
                    proofCancelled = true
                    return
                }
                if progress.solved {
                    session = nil
                    proofReady = true
                    await publish(attested: attested)
                    return
                }
            }
        }
    }

    @MainActor
    private func cancelProof() {
        guard isSolving && !isPublishing else { return }
        cancelRequested = true
        session?.cancel()
    }

    @MainActor
    private func publish(attested: Bool) async {
        isPublishing = true
        let expectedOpening = opening
        let selected = candidate
        let selectedPeer = peerId
        let outcome = await IdentityIO.performProfile { () -> ProfilePublishAttempt in
            do {
                return .published(
                    try RustBridge.publishProfile(
                        for: selected, peerId: selectedPeer,
                        attested: attested))
            } catch {
                return .failed(publicationErrorKey(error))
            }
        }
        guard opening == expectedOpening else { return }
        isPublishing = false
        isSolving = false
        guard !Task.isCancelled else { return }
        switch outcome {
        case .published(let snapshot):
            do {
                let draft = snapshot.draft
                publishedPreview = try ProfilePublicPreview(
                    draft: draft.values, country: draft.country)
                published = snapshot
            } catch {
                publishErrorKey = "profile.publishError.readback"
            }

        case .failed(let key):
            publishErrorKey = key
        }
    }

    private func publishedReadback(
        _ snapshot: PublishedProfileSnapshot,
        preview: ProfilePublicPreview
    ) -> some View {
        VStack(alignment: .leading) {
            Text("profile.published")
            HStack {
                Text("profile.publishedVersion")
                Text(verbatim: String(snapshot.version))
            }
            if preview.fields.isEmpty
                && preview.birth == ProfileBirthParts(year: nil, month: nil, day: nil)
            {
                Text("profile.noPublicFields")
            }
            ForEach(preview.fields, id: \.scaleKey) { field in
                HStack {
                    // An interpolated key would look up "profile.field.%@"; concatenate instead.
                    Text(LocalizedStringKey("profile.field." + field.scaleKey))
                    Text(
                        LocalizedStringKey(
                            "profile.value." + field.scaleKey + "." + field.choiceKey))
                }
            }
            if let year = preview.birth.year { birthRow("profile.birthYear", value: year) }
            if let month = preview.birth.month { birthRow("profile.birthMonth", value: month) }
            if let day = preview.birth.day { birthRow("profile.birthDay", value: day) }
            HStack {
                Text("profile.age")
                if let youngest = preview.youngest, let oldest = preview.oldest {
                    Text(verbatim: youngest == oldest ? String(youngest) : "\(youngest)-\(oldest)")
                } else {
                    Text("profile.ageUnknown")
                }
            }
            Text("profile.publicDisclosure")
            if preview.birth.year != nil && preview.birth.month != nil && preview.birth.day != nil {
                Text("profile.fullDateDisclosure")
            }
        }
        .accessibilityIdentifier("profile-published-readback")
    }

    private func birthRow(_ key: String, value: UInt32) -> some View {
        HStack {
            Text(LocalizedStringKey(key))
            Text(verbatim: String(value))
        }
    }
}
