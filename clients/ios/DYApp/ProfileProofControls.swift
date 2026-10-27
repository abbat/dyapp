import SwiftUI

private enum ProfilePublishAttempt: Sendable {
    case published(PublishedProfileSnapshot)
    case failed(String)
}

private func publicationErrorKey(_ error: Error) -> String {
    let message = String(describing: error)
    if message.contains("blocked: underage") { return "profile.publishError.underage" }
    if message.contains("blocked: notAttested") { return "profile.publishError.notAttested" }
    if message.contains("blocked: invalidBirthDate") { return "profile.publishError.invalidBirthDate" }
    if message.contains("notConnected") { return "profile.publishError.notConnected" }
    if message.contains("unsolved") { return "profile.publishError.unsolved" }
    if message.contains("readback differs") { return "profile.publishError.readback" }
    return "profile.publishError.generic"
}

/// Prepare the saved profile proof in cancellable, persisted Rust steps.
struct ProfileProofControls: View {
    let candidate: IdentityCandidate
    let peerId: String
    let canPrepare: Bool
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
    @State private var publishErrorKey: String?
    @State private var published: PublishedProfileSnapshot?
    @State private var publishedPreview: ProfilePublicPreview?

    var body: some View {
        Group {
            Button(LocalizedStringKey(publishErrorKey != nil ? "profile.retryPublish" :
                proofCancelled || proofFailed ? "profile.retryProof" : "profile.publish")) {
                Task { await prepareProof() }
            }
            .disabled(!canPrepare)
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
        }
        .onChange(of: resetToken) { _ in resetProofState() }
        .onDisappear(perform: cancelProof)
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
    private func prepareProof() async {
        guard canPrepare else { return }
        let confirmedAdult = attested
        clearConfirmations()
        isSolving = true
        cancelRequested = false
        resetProofState()
        let selected = candidate
        let selectedPeer = peerId
        let opened = await Task.detached(priority: .userInitiated) {
            try? RustBridge.profileSolveSession(for: selected, peerId: selectedPeer)
        }.value
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
        session = opened
        if cancelRequested { opened.cancel() }
        await runSteps(opened, attested: confirmedAdult)
    }

    @MainActor
    private func runSteps(_ opened: ProfileSolveSession, attested: Bool) async {
        while true {
            let result = await opened.runStep(attested: attested)
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
        let selected = candidate
        let selectedPeer = peerId
        let outcome = await Task.detached(priority: .userInitiated) { () -> ProfilePublishAttempt in
            do {
                return .published(try RustBridge.publishProfile(for: selected, peerId: selectedPeer,
                                                                attested: attested))
            } catch {
                return .failed(publicationErrorKey(error))
            }
        }.value
        isPublishing = false
        isSolving = false
        guard !Task.isCancelled else { return }
        switch outcome {
        case .published(let snapshot):
            do {
                publishedPreview = try ProfilePublicPreview(draft: snapshot.values)
                published = snapshot
            } catch {
                publishErrorKey = "profile.publishError.readback"
            }

        case .failed(let key):
            publishErrorKey = key
        }
    }

    private func publishedReadback(_ snapshot: PublishedProfileSnapshot,
                                   preview: ProfilePublicPreview) -> some View {
        VStack(alignment: .leading) {
            Text("profile.published")
            HStack {
                Text("profile.publishedVersion")
                Text(verbatim: String(snapshot.version))
            }
            if preview.fields.isEmpty && preview.birth == ProfileBirthParts(year: nil, month: nil, day: nil) {
                Text("profile.noPublicFields")
            }
            ForEach(preview.fields, id: \.scaleKey) { field in
                HStack {
                    Text(LocalizedStringKey("profile.field.\(field.scaleKey)"))
                    Text(LocalizedStringKey("profile.value.\(field.scaleKey).\(field.choiceKey)"))
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
