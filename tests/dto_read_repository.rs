use std::collections::HashSet;

use croisiere::{
    application::{Filter, Pagination, ReadRepository, Snapshot, Sorter, VersionedSnapshot},
    domain::Aggregate,
    infrastructure::persistence::{
        DTO, DTOReadRepository, DTORepository, DTORepositoryError, DTOSelect,
    },
};

#[derive(Debug, Default)]
struct TestAggregate(u64);

impl Aggregate for TestAggregate {
    const NAME: &'static str = "TestAggregate";
    type Error = std::convert::Infallible;
    type Event = ();
}

#[derive(Debug, Clone, Snapshot)]
#[snapshot(aggregate = TestAggregate, filter = TestFilter, sorter = TestSorter)]
struct TestSnapshot(u64);

impl From<TestAggregate> for TestSnapshot {
    fn from(value: TestAggregate) -> Self {
        Self(value.0)
    }
}

impl From<TestSnapshot> for TestAggregate {
    fn from(value: TestSnapshot) -> Self {
        Self(value.0)
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Filter)]
struct TestFilter(u64);

#[derive(Debug, PartialEq, Eq, Sorter)]
struct TestSorter;

#[derive(Debug, Hash, PartialEq, Eq)]
struct TestDTOFilter(u64);

impl From<TestFilter> for TestDTOFilter {
    fn from(value: TestFilter) -> Self {
        Self(value.0 * 10)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum TestDTOSorter {
    ByValue,
}

impl From<TestSorter> for TestDTOSorter {
    fn from(_: TestSorter) -> Self {
        Self::ByValue
    }
}

#[derive(Debug, Clone)]
struct TestDTO {
    value: u64,
    version: u128,
    valid: bool,
}

impl DTO for TestDTO {
    const NAME: &'static str = "TestDTO";

    type ConnectionData = TestConnection;
    type Error = &'static str;
}

impl From<VersionedSnapshot<TestSnapshot>> for TestDTO {
    fn from(value: VersionedSnapshot<TestSnapshot>) -> Self {
        Self {
            value: value.snapshot.0,
            version: value.version,
            valid: true,
        }
    }
}

impl TryFrom<TestDTO> for VersionedSnapshot<TestSnapshot> {
    type Error = &'static str;

    fn try_from(value: TestDTO) -> Result<Self, Self::Error> {
        if !value.valid {
            return Err("invalid DTO");
        }
        Ok(Self {
            snapshot: TestSnapshot(value.value),
            version: value.version,
        })
    }
}

#[derive(Default)]
struct TestConnection {
    dtos: Vec<TestDTO>,
    error: Option<&'static str>,
}

impl DTOSelect for TestDTO {
    type Filter = TestDTOFilter;
    type Sorter = TestDTOSorter;

    fn select(
        conn: &mut TestConnection,
        filters: HashSet<Self::Filter>,
        sorter: Option<Self::Sorter>,
        pagination: Option<Pagination>,
    ) -> Result<Vec<Self>, <Self as DTO>::Error> {
        assert_eq!(filters, HashSet::from([TestDTOFilter(70)]));
        assert_eq!(sorter, Some(TestDTOSorter::ByValue));
        let pagination = pagination.unwrap();
        assert_eq!((pagination.limit, pagination.offset), (2, 3));
        conn.error.map_or_else(|| Ok(conn.dtos.clone()), Err)
    }
}

#[derive(Default)]
struct TestRepository {
    dtos: Vec<TestDTO>,
    error: Option<&'static str>,
    connection_error: Option<&'static str>,
}

impl DTORepository for TestRepository {
    const NAME: &'static str = "TestRepository";

    type ConnectionData = TestConnection;
    type Error = &'static str;

    fn connection(&mut self) -> Result<Self::ConnectionData, Self::Error> {
        if let Some(error) = self.connection_error {
            return Err(error);
        }
        Ok(TestConnection {
            dtos: self.dtos.clone(),
            error: self.error,
        })
    }
}

impl DTOReadRepository<TestSnapshot> for TestRepository {
    type DTO = TestDTO;

    fn count(
        &mut self,
        filters: HashSet<TestFilter>,
        sorter: Option<TestSorter>,
    ) -> Result<usize, Self::Error> {
        assert_eq!(filters, HashSet::from([TestFilter(7)]));
        assert_eq!(sorter, Some(TestSorter));
        self.error.map_or(Ok(42), Err)
    }
}

#[derive(Debug, Clone, Snapshot)]
#[snapshot(aggregate = TestAggregate, filter = AlternateFilter, sorter = AlternateSorter)]
struct AlternateSnapshot(u64);

impl From<TestAggregate> for AlternateSnapshot {
    fn from(value: TestAggregate) -> Self {
        Self(value.0)
    }
}

impl From<AlternateSnapshot> for TestAggregate {
    fn from(value: AlternateSnapshot) -> Self {
        Self(value.0)
    }
}

#[derive(Hash, PartialEq, Eq, Filter)]
struct AlternateFilter(u64);

#[derive(Sorter)]
struct AlternateSorter;

impl From<AlternateFilter> for TestDTOFilter {
    fn from(value: AlternateFilter) -> Self {
        Self(value.0)
    }
}

impl From<AlternateSorter> for TestDTOSorter {
    fn from(_: AlternateSorter) -> Self {
        Self::ByValue
    }
}

impl From<VersionedSnapshot<AlternateSnapshot>> for TestDTO {
    fn from(value: VersionedSnapshot<AlternateSnapshot>) -> Self {
        Self {
            value: value.snapshot.0,
            version: value.version,
            valid: true,
        }
    }
}

impl TryFrom<TestDTO> for VersionedSnapshot<AlternateSnapshot> {
    type Error = &'static str;

    fn try_from(value: TestDTO) -> Result<Self, Self::Error> {
        let snapshot = VersionedSnapshot::<TestSnapshot>::try_from(value)?;
        Ok(Self {
            snapshot: AlternateSnapshot(snapshot.snapshot.0),
            version: snapshot.version,
        })
    }
}

impl DTOReadRepository<AlternateSnapshot> for TestRepository {
    type DTO = TestDTO;

    fn count(
        &mut self,
        filters: HashSet<AlternateFilter>,
        sorter: Option<AlternateSorter>,
    ) -> Result<usize, Self::Error> {
        assert!(filters == HashSet::from([AlternateFilter(70)]));
        assert!(sorter.is_some());
        self.error.map_or(Ok(42), Err)
    }
}

fn get(
    repository: &mut TestRepository,
) -> Result<Vec<VersionedSnapshot<TestSnapshot>>, DTORepositoryError<&'static str, &'static str>> {
    ReadRepository::get(
        repository,
        HashSet::from([TestFilter(7)]),
        Some(TestSorter),
        Some(Pagination {
            limit: 2,
            offset: 3,
        }),
    )
}

#[test]
fn one_dto_supports_multiple_snapshots_and_their_selection_criteria() {
    let primary = VersionedSnapshot {
        snapshot: TestSnapshot(9),
        version: 12,
    };
    let alternate = VersionedSnapshot {
        snapshot: AlternateSnapshot(4),
        version: 8,
    };
    let mut repository = TestRepository {
        dtos: vec![primary.into(), alternate.into()],
        ..Default::default()
    };

    let primary_results = get(&mut repository).unwrap();
    let alternate_results = ReadRepository::<AlternateSnapshot>::get(
        &mut repository,
        HashSet::from([AlternateFilter(70)]),
        Some(AlternateSorter),
        Some(Pagination {
            limit: 2,
            offset: 3,
        }),
    )
    .unwrap();

    let primary_values: Vec<_> = primary_results
        .into_iter()
        .map(|s| (s.snapshot.0, s.version))
        .collect();
    let alternate_values: Vec<_> = alternate_results
        .into_iter()
        .map(|s| (s.snapshot.0, s.version))
        .collect();
    assert_eq!(primary_values, vec![(9, 12), (4, 8)]);
    assert_eq!(alternate_values, primary_values);
}

#[test]
fn get_forwards_query_and_preserves_order_values_and_versions() {
    let mut repository = TestRepository {
        dtos: vec![
            TestDTO {
                value: 9,
                version: 12,
                valid: true,
            },
            TestDTO {
                value: 4,
                version: 8,
                valid: true,
            },
        ],
        ..Default::default()
    };

    let snapshots = get(&mut repository).unwrap();
    let values: Vec<_> = snapshots
        .into_iter()
        .map(|s| (s.snapshot.0, s.version))
        .collect();
    assert_eq!(values, vec![(9, 12), (4, 8)]);
}

#[test]
fn get_returns_an_empty_collection_when_no_dtos_match() {
    assert!(get(&mut TestRepository::default()).unwrap().is_empty());
}

#[test]
fn get_preserves_storage_errors() {
    let mut repository = TestRepository {
        error: Some("storage unavailable"),
        ..Default::default()
    };
    assert!(matches!(
        get(&mut repository),
        Err(DTORepositoryError::Repository("storage unavailable"))
    ));
}

#[test]
fn get_preserves_connection_errors() {
    let mut repository = TestRepository {
        connection_error: Some("connection unavailable"),
        ..Default::default()
    };
    assert!(matches!(
        get(&mut repository),
        Err(DTORepositoryError::Repository("connection unavailable"))
    ));
}

#[test]
fn get_returns_conversion_errors_instead_of_partial_results() {
    let mut repository = TestRepository {
        dtos: vec![
            TestDTO {
                value: 9,
                version: 12,
                valid: true,
            },
            TestDTO {
                value: 4,
                version: 8,
                valid: false,
            },
        ],
        ..Default::default()
    };
    assert!(matches!(
        get(&mut repository),
        Err(DTORepositoryError::Conversion("invalid DTO"))
    ));
}

#[test]
fn count_forwards_query_and_preserves_storage_errors() {
    let mut repository = TestRepository::default();
    assert_eq!(
        ReadRepository::<TestSnapshot>::count(
            &mut repository,
            HashSet::from([TestFilter(7)]),
            Some(TestSorter)
        )
        .unwrap(),
        42
    );

    repository.error = Some("count unavailable");
    assert!(matches!(
        ReadRepository::<TestSnapshot>::count(
            &mut repository,
            HashSet::from([TestFilter(7)]),
            Some(TestSorter)
        ),
        Err(DTORepositoryError::Repository("count unavailable"))
    ));
}
