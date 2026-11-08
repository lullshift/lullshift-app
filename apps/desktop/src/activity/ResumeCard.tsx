import { relativeTime } from "../format";
import { Icon } from "../ui/Icon";
import type { ActivityInfo } from "./ActivityHost";
import { AiTag } from "./GeneratedBadge";
import { helpLabel, kindLabel } from "./library";
import { usePreview } from "./usePreview";

interface Props {
  activity: ActivityInfo;
  now: number;
  onResume: () => void;
  onStartOver?: () => void;
  onHelp?: () => void;
}

/**
 * A saved activity: its name, what kind it is and how long ago it was saved, with Resume.
 * Shows no play counts, totals or streaks (ADR-026).
 */
export function ResumeCard({ activity: a, now, onResume, onStartOver, onHelp }: Props) {
  const preview = usePreview(a.id, a.preview);
  return (
    <li className="card resume-card">
      <div className="resume-thumb" aria-hidden="true">
        {preview ? <img src={preview} alt="" /> : <Icon name="resume" size={22} />}
      </div>
      <div className="resume-main">
        <h3 className="resume-name">{a.name}</h3>
        <p className="resume-meta">
          {kindLabel(a.category)}
          {a.savedUtc ? ` · saved ${relativeTime(a.savedUtc, now)}` : ""}
        </p>
        <AiTag info={a} />
      </div>
      <div className="resume-actions">
        <button type="button" className="btn btn-primary" onClick={onResume}>
          <Icon name="play" size={18} />
          Resume
          <span className="visually-hidden"> {a.name}</span>
        </button>
        {onStartOver && (
          <button type="button" className="btn btn-quiet" onClick={onStartOver}>
            Start over
            <span className="visually-hidden"> {a.name}</span>
          </button>
        )}
        {onHelp && (
          <button type="button" className="btn btn-quiet" onClick={onHelp}>
            {helpLabel(a.category)}
            <span className="visually-hidden"> {a.name}</span>
          </button>
        )}
      </div>
    </li>
  );
}
