import SwiftUI

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

    var body: some View {
        Group {
            Button(LocalizedStringKey(proofCancelled || proofFailed ?
                "profile.retryProof" : "profile.preparePublish")) {
                Task { await prepareProof() }
            }
            .disabled(!canPrepare)
            .accessibilityIdentifier("profile-prepare-publish")
            if isSolving {
                ProgressView()
                Button("profile.cancelProof") { cancelProof() }
                    .disabled(cancelRequested)
                    .accessibilityIdentifier("profile-cancel-proof")
            }
            if nextNonce > 0 {
                HStack {
                    Text("profile.proofNonce")
                    Spacer()
                    Text(verbatim: String(nextNonce))
                }
                .accessibilityIdentifier("profile-proof-progress")
            }
            if proofReady { Text("profile.proofReady") }
            if proofCancelled { Text("profile.proofCancelled") }
            if proofFailed { Text("profile.proofFailed") }
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
                if progress.solved {
                    session = nil
                    isSolving = false
                    proofReady = true
                    return
                }
                if progress.cancelled || cancelRequested {
                    session = nil
                    isSolving = false
                    proofCancelled = true
                    return
                }
            }
        }
    }

    @MainActor
    private func cancelProof() {
        guard isSolving else { return }
        cancelRequested = true
        session?.cancel()
    }
}
