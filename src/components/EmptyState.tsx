export function EmptyState({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="empty-state">
      <h2>Пока нет ни одного бэкапа под присмотром</h2>
      <p>
        Укажите папку с копией — на внешнем диске, в сети или в облачной синхронизации. Программа сама
        будет проверять, что она на месте, обновляется и не портится, и предупредит, только если что-то не так.
      </p>
      <button className="btn btn-primary" onClick={onAdd}>
        Добавить первый бэкап
      </button>
    </div>
  );
}
