import { describe, expect, it } from "vitest";
import { withoutInFlightMessage } from "./logic";

const m = (id: number, role: string, status: string) => ({ id, role, status });

describe("withoutInFlightMessage", () => {
  it("leaves out the stored pending copy of the message being sent", () => {
    const list = [m(1, "user", "sent"), m(2, "assistant", "sent"), m(3, "user", "pending")];
    expect(withoutInFlightMessage(list, "hello").map((x) => x.id)).toEqual([1, 2]);
  });

  it("changes nothing for a retry (no typed text): the failed message stays and shows Sending again", () => {
    const list = [m(1, "user", "pending")];
    expect(withoutInFlightMessage(list, null)).toBe(list);
  });

  it("changes nothing when the last message is not a pending one of yours", () => {
    const sent = [m(1, "user", "sent")];
    expect(withoutInFlightMessage(sent, "x")).toBe(sent);
    const failed = [m(1, "user", "failed")];
    expect(withoutInFlightMessage(failed, "x")).toBe(failed);
    const reply = [m(1, "user", "pending"), m(2, "assistant", "pending")];
    expect(withoutInFlightMessage(reply, "x")).toBe(reply);
  });

  it("copes with an empty thread", () => {
    expect(withoutInFlightMessage([], "x")).toEqual([]);
  });
});
