import type { SummaryItem } from "@lullshift/activity-sdk/host";
import { useId } from "react";
import { truncate } from "../marketplace/logic";
import { formatPlayed } from "./playTime";

/**
 * What the paused view shows instead of an empty card (S05-T09): time played, always; the
 * activity's own stats if it reported any; otherwise its preview image.
 *
 * Host stats and activity stats are separate groups. The activity's labels are its own words, so
 * they sit under a heading with the activity's name and can't pass as a host stat.
 */
export function PausedStats({
  name,
  summary,
  playedNow,
  playedTotal,
  preview,
}: {
  name: string;
  summary: SummaryItem[] | null;
  playedNow: number;
  playedTotal: number;
  preview: string | null;
}) {
  const headingId = useId();
  const showTotal = Math.floor(playedTotal / 60) > Math.floor(playedNow / 60);
  const own = summary && summary.length > 0 ? summary : null;
  return (
    <div className="paused-stats">
      <dl className="stat-list">
        <div className="stat">
          <dt>Played this time</dt>
          <dd>{formatPlayed(playedNow)}</dd>
        </div>
        {showTotal && (
          <div className="stat">
            <dt>In total</dt>
            <dd>{formatPlayed(playedTotal)}</dd>
          </div>
        )}
      </dl>
      {own && (
        <section className="stat-group" aria-labelledby={headingId}>
          <h3 id={headingId} className="stat-group-title">
            From {truncate(name, 60)}
          </h3>
          <dl className="stat-list">
            {own.map((item, i) => (
              <div className="stat" key={i}>
                <dt>{item.label}</dt>
                <dd>{String(item.value)}</dd>
              </div>
            ))}
          </dl>
        </section>
      )}
      {!summary && preview && (
        <img className="paused-preview" src={preview} alt={`${name} preview`} />
      )}
    </div>
  );
}
